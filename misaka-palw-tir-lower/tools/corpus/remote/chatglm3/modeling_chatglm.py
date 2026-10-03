"""A RECONSTRUCTION of THUDM/chatglm3-6b's `modeling_chatglm.py` (the PyTorch >= 2 inference path), not the repository's file.

Written from the documented forward of ChatGLM3: an embedding; rotary position embeddings over the first half of each head's channels, applied to ADJACENT pairs
(`rotary_dim // 2` frequencies of base 10000 * rope_ratio); per layer `input_layernorm` (RMSNorm or LayerNorm), multi-query attention with a fused `query_key_value`
(rows `[q | k | v]` with `multi_query_group_num` key/value groups, or per-head `[q_h | k_h | v_h]` without), `dense`, `post_attention_layernorm`, and a SwiGLU MLP
(`dense_h_to_4h` rows `[gate | up]`, `dense_4h_to_h`); a final norm; `output_layer`. Tensor and module names follow the repository's checkpoint
(`transformer.embedding.word_embeddings`, `transformer.encoder.layers.N.*`, `transformer.encoder.final_layernorm`, `transformer.output_layer`).

Nothing here is verified against the repository: the lowering follows the repository's source, which this file stands in for.
"""

import math

import torch
import torch.nn.functional as F
from torch import nn
from transformers import PreTrainedModel

from .configuration_chatglm import ChatGLMConfig


class RMSNorm(nn.Module):
    def __init__(self, normalized_shape, eps=1e-5, **kwargs):
        super().__init__()
        self.weight = nn.Parameter(torch.ones(normalized_shape))
        self.eps = eps

    def forward(self, hidden_states: torch.Tensor):
        input_dtype = hidden_states.dtype
        variance = hidden_states.to(torch.float32).pow(2).mean(-1, keepdim=True)
        hidden_states = hidden_states * torch.rsqrt(variance + self.eps)
        return (self.weight * hidden_states).to(input_dtype)


class RotaryEmbedding(nn.Module):
    def __init__(self, dim, rope_ratio=1, original_impl=False, **kwargs):
        super().__init__()
        inv_freq = 1.0 / (10000 ** (torch.arange(0, dim, 2).to(dtype=torch.float32) / dim))
        self.register_buffer("inv_freq", inv_freq)
        self.dim = dim
        self.original_impl = original_impl
        self.rope_ratio = rope_ratio

    def forward_impl(self, seq_len, n_elem, dtype, device, base=10000):
        base = base * self.rope_ratio
        theta = 1.0 / (base ** (torch.arange(0, n_elem, 2, dtype=torch.float, device=device) / n_elem))
        seq_idx = torch.arange(seq_len, dtype=torch.float, device=device)
        idx_theta = torch.outer(seq_idx, theta).float()
        cache = torch.stack([torch.cos(idx_theta), torch.sin(idx_theta)], dim=-1)
        # the table is stored in the checkpoint's dtype (half precision in the release)
        if dtype in (torch.float16, torch.bfloat16, torch.int8):
            cache = cache.bfloat16() if dtype == torch.bfloat16 else cache.half()
        return cache

    def forward(self, max_seq_len, offset=0):
        return self.forward_impl(max_seq_len, self.dim, dtype=self.inv_freq.dtype, device=self.inv_freq.device)


def apply_rotary_pos_emb(x: torch.Tensor, rope_cache: torch.Tensor) -> torch.Tensor:
    # x: [sq, b, np, hn]
    sq, b, np_, hn = x.size(0), x.size(1), x.size(2), x.size(3)
    rot_dim = rope_cache.shape[-2] * 2
    x, x_pass = x[..., :rot_dim], x[..., rot_dim:]
    rope_cache = rope_cache[:sq]
    xshaped = x.reshape(sq, -1, np_, rot_dim // 2, 2)
    rope_cache = rope_cache.view(sq, -1, 1, xshaped.size(3), 2)
    x_out2 = torch.stack(
        [
            xshaped[..., 0] * rope_cache[..., 0] - xshaped[..., 1] * rope_cache[..., 1],
            xshaped[..., 1] * rope_cache[..., 0] + xshaped[..., 0] * rope_cache[..., 1],
        ],
        -1,
    )
    x_out2 = x_out2.flatten(3)
    return torch.cat((x_out2, x_pass), dim=-1)


class CoreAttention(nn.Module):
    def __init__(self, config: ChatGLMConfig, layer_number):
        super().__init__()
        self.apply_query_key_layer_scaling = config.apply_query_key_layer_scaling
        self.attention_softmax_in_fp32 = config.attention_softmax_in_fp32
        if self.apply_query_key_layer_scaling:
            self.attention_softmax_in_fp32 = True
        self.layer_number = max(1, layer_number)
        projection_size = config.kv_channels * config.num_attention_heads
        self.hidden_size_per_partition = projection_size
        self.hidden_size_per_attention_head = projection_size // config.num_attention_heads
        self.num_attention_heads_per_partition = config.num_attention_heads
        coeff = None
        self.norm_factor = math.sqrt(self.hidden_size_per_attention_head)
        if self.apply_query_key_layer_scaling:
            coeff = self.layer_number
            self.norm_factor *= coeff
        self.coeff = coeff
        self.attention_dropout = nn.Dropout(config.attention_dropout)

    def forward(self, query_layer, key_layer, value_layer, attention_mask):
        # PyTorch >= 2: the fused kernel scales by 1/sqrt(head_dim) (the layer-number coefficient of the older path cancels).
        query_layer, key_layer, value_layer = [k.permute(1, 2, 0, 3) for k in [query_layer, key_layer, value_layer]]
        if attention_mask is None and query_layer.shape[2] == key_layer.shape[2]:
            context_layer = F.scaled_dot_product_attention(query_layer, key_layer, value_layer, is_causal=True)
        else:
            if attention_mask is not None:
                attention_mask = ~attention_mask
            context_layer = F.scaled_dot_product_attention(query_layer, key_layer, value_layer, attention_mask)
        context_layer = context_layer.permute(2, 0, 1, 3)
        new_context_layer_shape = context_layer.size()[:-2] + (self.hidden_size_per_partition,)
        return context_layer.reshape(*new_context_layer_shape)


class SelfAttention(nn.Module):
    def __init__(self, config: ChatGLMConfig, layer_number):
        super().__init__()
        self.layer_number = max(1, layer_number)
        self.projection_size = config.kv_channels * config.num_attention_heads
        self.hidden_size_per_attention_head = self.projection_size // config.num_attention_heads
        self.num_attention_heads_per_partition = config.num_attention_heads
        self.multi_query_attention = config.multi_query_attention
        self.qkv_hidden_size = 3 * self.projection_size
        if self.multi_query_attention:
            self.num_multi_query_groups_per_partition = config.multi_query_group_num
            self.qkv_hidden_size = self.projection_size + 2 * self.hidden_size_per_attention_head * config.multi_query_group_num
        self.query_key_value = nn.Linear(config.hidden_size, self.qkv_hidden_size, bias=config.add_bias_linear or config.add_qkv_bias)
        self.core_attention = CoreAttention(config, self.layer_number)
        self.dense = nn.Linear(self.projection_size, config.hidden_size, bias=config.add_bias_linear)

    def forward(self, hidden_states, attention_mask, rotary_pos_emb):
        mixed_x_layer = self.query_key_value(hidden_states)
        np_, hn = self.num_attention_heads_per_partition, self.hidden_size_per_attention_head
        if self.multi_query_attention:
            g = self.num_multi_query_groups_per_partition
            query_layer, key_layer, value_layer = mixed_x_layer.split([np_ * hn, g * hn, g * hn], dim=-1)
            query_layer = query_layer.view(query_layer.size()[:-1] + (np_, hn))
            key_layer = key_layer.view(key_layer.size()[:-1] + (g, hn))
            value_layer = value_layer.view(value_layer.size()[:-1] + (g, hn))
        else:
            mixed_x_layer = mixed_x_layer.view(mixed_x_layer.size()[:-1] + (np_, 3 * hn))
            query_layer, key_layer, value_layer = [t.contiguous() for t in torch.chunk(mixed_x_layer, 3, dim=-1)]
        if rotary_pos_emb is not None:
            query_layer = apply_rotary_pos_emb(query_layer, rotary_pos_emb)
            key_layer = apply_rotary_pos_emb(key_layer, rotary_pos_emb)
        if self.multi_query_attention:
            g = self.num_multi_query_groups_per_partition
            key_layer = key_layer.unsqueeze(-2).expand(-1, -1, -1, np_ // g, -1)
            key_layer = key_layer.contiguous().view(key_layer.size()[:2] + (np_, hn))
            value_layer = value_layer.unsqueeze(-2).expand(-1, -1, -1, np_ // g, -1)
            value_layer = value_layer.contiguous().view(value_layer.size()[:2] + (np_, hn))
        context_layer = self.core_attention(query_layer, key_layer, value_layer, attention_mask)
        return self.dense(context_layer)


def swiglu(x):
    x = torch.chunk(x, 2, dim=-1)
    return F.silu(x[0]) * x[1]


class MLP(nn.Module):
    def __init__(self, config: ChatGLMConfig):
        super().__init__()
        self.add_bias = config.add_bias_linear
        self.dense_h_to_4h = nn.Linear(config.hidden_size, config.ffn_hidden_size * 2, bias=self.add_bias)
        self.activation_func = swiglu
        self.dense_4h_to_h = nn.Linear(config.ffn_hidden_size, config.hidden_size, bias=self.add_bias)

    def forward(self, hidden_states):
        return self.dense_4h_to_h(self.activation_func(self.dense_h_to_4h(hidden_states)))


class GLMBlock(nn.Module):
    def __init__(self, config: ChatGLMConfig, layer_number):
        super().__init__()
        self.layer_number = layer_number
        self.apply_residual_connection_post_layernorm = config.apply_residual_connection_post_layernorm
        self.fp32_residual_connection = config.fp32_residual_connection
        LayerNormFunc = RMSNorm if config.rmsnorm else nn.LayerNorm
        self.input_layernorm = LayerNormFunc(config.hidden_size, eps=config.layernorm_epsilon)
        self.self_attention = SelfAttention(config, layer_number)
        self.hidden_dropout = config.hidden_dropout
        self.post_attention_layernorm = LayerNormFunc(config.hidden_size, eps=config.layernorm_epsilon)
        self.mlp = MLP(config)

    def forward(self, hidden_states, attention_mask, rotary_pos_emb):
        layernorm_output = self.input_layernorm(hidden_states)
        attention_output = self.self_attention(layernorm_output, attention_mask, rotary_pos_emb)
        residual = layernorm_output if self.apply_residual_connection_post_layernorm else hidden_states
        layernorm_input = residual + F.dropout(attention_output, p=self.hidden_dropout, training=self.training)
        layernorm_output = self.post_attention_layernorm(layernorm_input)
        mlp_output = self.mlp(layernorm_output)
        residual = layernorm_output if self.apply_residual_connection_post_layernorm else layernorm_input
        return residual + F.dropout(mlp_output, p=self.hidden_dropout, training=self.training)


class GLMTransformer(nn.Module):
    def __init__(self, config: ChatGLMConfig):
        super().__init__()
        self.post_layer_norm = config.post_layer_norm
        self.num_layers = config.num_layers
        self.layers = nn.ModuleList([GLMBlock(config, i + 1) for i in range(self.num_layers)])
        if self.post_layer_norm:
            LayerNormFunc = RMSNorm if config.rmsnorm else nn.LayerNorm
            self.final_layernorm = LayerNormFunc(config.hidden_size, eps=config.layernorm_epsilon)

    def forward(self, hidden_states, attention_mask, rotary_pos_emb):
        for layer in self.layers:
            hidden_states = layer(hidden_states, attention_mask, rotary_pos_emb)
        if self.post_layer_norm:
            hidden_states = self.final_layernorm(hidden_states)
        return hidden_states


class Embedding(nn.Module):
    def __init__(self, config: ChatGLMConfig):
        super().__init__()
        self.hidden_size = config.hidden_size
        self.word_embeddings = nn.Embedding(config.padded_vocab_size, self.hidden_size)
        self.fp32_residual_connection = config.fp32_residual_connection

    def forward(self, input_ids):
        embeddings = self.word_embeddings(input_ids)
        embeddings = embeddings.transpose(0, 1).contiguous()
        if self.fp32_residual_connection:
            embeddings = embeddings.float()
        return embeddings


class ChatGLMPreTrainedModel(PreTrainedModel):
    config_class = ChatGLMConfig
    base_model_prefix = "transformer"
    _no_split_modules = ["GLMBlock"]

    def _init_weights(self, module):
        return


class ChatGLMModel(ChatGLMPreTrainedModel):
    def __init__(self, config: ChatGLMConfig):
        super().__init__(config)
        self.embedding = Embedding(config)
        self.num_layers = config.num_layers
        self.multi_query_group_num = config.multi_query_group_num
        self.kv_channels = config.kv_channels
        self.seq_length = config.seq_length
        rotary_dim = config.hidden_size // config.num_attention_heads if config.kv_channels is None else config.kv_channels
        self.rotary_pos_emb = RotaryEmbedding(rotary_dim // 2, rope_ratio=config.rope_ratio, original_impl=config.original_rope)
        self.encoder = GLMTransformer(config)
        self.output_layer = nn.Linear(config.hidden_size, config.padded_vocab_size, bias=False)
        self.post_init()

    def forward(self, input_ids):
        seq_length = input_ids.shape[1]
        hidden_states = self.embedding(input_ids)
        rotary_pos_emb = self.rotary_pos_emb(self.seq_length)
        rotary_pos_emb = rotary_pos_emb[None, :seq_length]
        rotary_pos_emb = rotary_pos_emb.transpose(0, 1).contiguous()
        return self.encoder(hidden_states, None, rotary_pos_emb)


class ChatGLMForConditionalGeneration(ChatGLMPreTrainedModel):
    def __init__(self, config: ChatGLMConfig):
        super().__init__(config)
        self.transformer = ChatGLMModel(config)
        self.config = config
        self.post_init()

    def forward(self, input_ids, **kwargs):
        hidden_states = self.transformer(input_ids)
        lm_logits = self.transformer.output_layer(hidden_states)
        lm_logits = lm_logits.transpose(0, 1).contiguous()
        return type("Out", (), {"logits": lm_logits})()
