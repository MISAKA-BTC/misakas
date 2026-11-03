---
library_name: transformers
license: apache-2.0
language:
- en
---


# SmolLM2


![image/png](https://cdn-uploads.huggingface.co/production/uploads/61c141342aac764ce1654e43/gWt7M-JN62oXRpO-nQGo_.png)

##  Table of Contents

1. [Model Summary](##model-summary)
2. [Limitations](##limitations)
3. [Training](##training)
4. [License](##license)
5. [Citation](##citation)

## Model Summary

SmolLM2 is a family of compact language models available in three size: 135M, 360M, and 1.7B parameters. They are capable of solving a wide range of tasks while being lightweight enough to run on-device. More details in our paper: https://arxiv.org/abs/2502.02737

SmolLM2 demonstrates significant advances over its predecessor SmolLM1, particularly in instruction following, knowledge, reasoning. The 360M model was trained on 4 trillion tokens using a diverse dataset combination: FineWeb-Edu, DCLM, The Stack, along with new filtered datasets we curated and will release soon.  We developed the instruct version through supervised fine-tuning (SFT) using a combination of public datasets and our own curated datasets. We then applied Direct Preference Optimization (DPO) using [UltraFeedback](https://huggingface.co/datasets/HuggingFaceH4/ultrafeedback_binarized).

The instruct model additionally supports tasks such as text rewriting, summarization and function calling thanks to datasets developed by [Argilla](https://huggingface.co/argilla) such as [Synth-APIGen-v0.1](https://huggingface.co/datasets/argilla/Synth-APIGen-v0.1).

For more details refer to: https://github.com/huggingface/smollm. You will find pre-training, post-training, evaluation and local inference code.

### How to use

```bash
pip install transformers
```

#### Running the model on CPU/GPU/multi GPU
* _Using full precision_
```python
# pip install transformers
from transformers import AutoModelForCausalLM, AutoTokenizer
checkpoint = "HuggingFaceTB/SmolLM2-360M"
device = "cuda" # for GPU usage or "cpu" for CPU usage
tokenizer = AutoTokenizer.from_pretrained(checkpoint)
# for multiple GPUs install accelerate and do `model = AutoModelForCausalLM.from_pretrained(checkpoint, device_map="auto")`
model = AutoModelForCausalLM.from_pretrained(checkpoint).to(device)
inputs = tokenizer.encode("Gravity is", return_tensors="pt").to(device)
outputs = model.generate(inputs)
print(tokenizer.decode(outputs[0]))
```

* _Using `torch.bfloat16`_
```python
# pip install accelerate
import torch
from transformers import AutoTokenizer, AutoModelForCausalLM
checkpoint = "HuggingFaceTB/SmolLM2-360M"
tokenizer = AutoTokenizer.from_pretrained(checkpoint)
# for fp16 use `torch_dtype=torch.float16` instead
model = AutoModelForCausalLM.from_pretrained(checkpoint, device_map="auto", torch_dtype=torch.bfloat16)
inputs = tokenizer.encode("Gravity is", return_tensors="pt").to("cuda")
outputs = model.generate(inputs)
print(tokenizer.decode(outputs[0]))
```
```bash
>>> print(f"Memory footprint: {model.get_memory_footprint() / 1e6:.2f} MB")
Memory footprint: 723.56 MB
```

## Evaluation

In this section, we report the evaluation results of SmolLM2. All evaluations are zero-shot unless stated otherwise, and we use [lighteval](https://github.com/huggingface/lighteval) to run them.

## Base Pre-Trained Model

| Metrics            | SmolLM2-360M | Qwen2.5-0.5B | SmolLM-360M  |
|:-------------------|:------------:|:------------:|:------------:|
| HellaSwag          | **54.5**     | 51.2         | 51.8         |
| ARC (Average)      | **53.0**     | 45.4         | 50.1         |
| PIQA               | **71.7**     | 69.9         | 71.6         |
| MMLU (cloze)       | **35.8**     | 33.7         | 34.4         |
| CommonsenseQA      | **38.0**     | 31.6         | 35.3         |
| TriviaQA           | **16.9**     | 4.3          | 9.1          |
| Winogrande         | 52.5         | **54.1**     | 52.8         |
| OpenBookQA         | **37.4**     | **37.4**     | 37.2         |
| GSM8K (5-shot)     | 3.2          | **33.4**     | 1.6          |


## Instruction Model

| Metric                       | SmolLM2-360M-Instruct | Qwen2.5-0.5B-Instruct | SmolLM-360M-Instruct |
|:-----------------------------|:---------------------:|:---------------------:|:---------------------:|
| IFEval (Average prompt/inst) | **41.0**             | 31.6                 | 19.8                 |
| MT-Bench                     | 3.66                 | **4.16**             | 3.37                 |
| HellaSwag                    | **52.1**             | 48.0                 | 47.9                 |
| ARC (Average)                | **43.7**             | 37.3                 | 38.8                 |
| PIQA                         | **70.8**             | 67.2                 | 69.4                 |
| MMLU (cloze)                 | **32.8**             | 31.7                 | 30.6                 |
| BBH (3-shot)                 | 27.3                 | **30.7**             | 24.4                 |
| GSM8K (5-shot)               | 7.43                 | **26.8**             | 1.36                 |


## Limitations

SmolLM2 models primarily understand and generate content in English. They can produce text on a variety of topics, but the generated content may not always be factually accurate, logically consistent, or free from biases present in the training data. These models should be used as assistive tools rather than definitive sources of information. Users should always verify important information and critically evaluate any generated content.

## Training

### Model

- **Architecture:** Transformer decoder
- **Pretraining tokens:** 4T
- **Precision:** bfloat16

### Hardware

- **GPUs:** 128 H100

### Software

- **Training Framework:** [nanotron](https://github.com/huggingface/nanotron/tree/main)

## License

[Apache 2.0](https://www.apache.org/licenses/LICENSE-2.0)

## Citation
```bash
@misc{allal2025smollm2smolgoesbig,
      title={SmolLM2: When Smol Goes Big -- Data-Centric Training of a Small Language Model}, 
      author={Loubna Ben Allal and Anton Lozhkov and Elie Bakouch and Gabriel Martín Blázquez and Guilherme Penedo and Lewis Tunstall and Andrés Marafioti and Hynek Kydlíček and Agustín Piqueres Lajarín and Vaibhav Srivastav and Joshua Lochner and Caleb Fahlgren and Xuan-Son Nguyen and Clémentine Fourrier and Ben Burtenshaw and Hugo Larcher and Haojun Zhao and Cyril Zakka and Mathieu Morlon and Colin Raffel and Leandro von Werra and Thomas Wolf},
      year={2025},
      eprint={2502.02737},
      archivePrefix={arXiv},
      primaryClass={cs.CL},
      url={https://arxiv.org/abs/2502.02737}, 
}
```