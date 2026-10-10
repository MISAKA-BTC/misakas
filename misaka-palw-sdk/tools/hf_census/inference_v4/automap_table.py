import collections, json, sys
d = json.load(open('automap.json'))
TASK = {
 'MODEL_FOR_CAUSAL_LM_MAPPING_NAMES': 'text-generation',
 'MODEL_FOR_MASKED_LM_MAPPING_NAMES': 'fill-mask',
 'MODEL_FOR_SEQ_TO_SEQ_CAUSAL_LM_MAPPING_NAMES': 'text2text-generation',
 'MODEL_FOR_SEQUENCE_CLASSIFICATION_MAPPING_NAMES': 'text-classification',
 'MODEL_FOR_TOKEN_CLASSIFICATION_MAPPING_NAMES': 'token-classification',
 'MODEL_FOR_QUESTION_ANSWERING_MAPPING_NAMES': 'question-answering',
 'MODEL_FOR_TABLE_QUESTION_ANSWERING_MAPPING_NAMES': 'table-question-answering',
 'MODEL_FOR_MULTIPLE_CHOICE_MAPPING_NAMES': 'multiple-choice',
 'MODEL_FOR_IMAGE_CLASSIFICATION_MAPPING_NAMES': 'image-classification',
 'MODEL_FOR_ZERO_SHOT_IMAGE_CLASSIFICATION_MAPPING_NAMES': 'zero-shot-image-classification',
 'MODEL_FOR_OBJECT_DETECTION_MAPPING_NAMES': 'object-detection',
 'MODEL_FOR_ZERO_SHOT_OBJECT_DETECTION_MAPPING_NAMES': 'zero-shot-object-detection',
 'MODEL_FOR_SEMANTIC_SEGMENTATION_MAPPING_NAMES': 'image-segmentation',
 'MODEL_FOR_IMAGE_SEGMENTATION_MAPPING_NAMES': 'image-segmentation',
 'MODEL_FOR_INSTANCE_SEGMENTATION_MAPPING_NAMES': 'image-segmentation',
 'MODEL_FOR_UNIVERSAL_SEGMENTATION_MAPPING_NAMES': 'image-segmentation',
 'MODEL_FOR_DEPTH_ESTIMATION_MAPPING_NAMES': 'depth-estimation',
 'MODEL_FOR_MASK_GENERATION_MAPPING_NAMES': 'mask-generation',
 'MODEL_FOR_KEYPOINT_DETECTION_MAPPING_NAMES': 'keypoint-detection',
 'MODEL_FOR_VIDEO_CLASSIFICATION_MAPPING_NAMES': 'video-classification',
 'MODEL_FOR_AUDIO_CLASSIFICATION_MAPPING_NAMES': 'audio-classification',
 'MODEL_FOR_CTC_MAPPING_NAMES': 'automatic-speech-recognition',
 'MODEL_FOR_SPEECH_SEQ_2_SEQ_MAPPING_NAMES': 'automatic-speech-recognition',
 'MODEL_FOR_TEXT_TO_WAVEFORM_MAPPING_NAMES': 'text-to-speech',
 'MODEL_FOR_TEXT_TO_SPECTROGRAM_MAPPING_NAMES': 'text-to-speech',
 'MODEL_FOR_IMAGE_TEXT_TO_TEXT_MAPPING_NAMES': 'image-text-to-text',
 'MODEL_FOR_VISUAL_QUESTION_ANSWERING_MAPPING_NAMES': 'visual-question-answering',
 'MODEL_FOR_DOCUMENT_QUESTION_ANSWERING_MAPPING_NAMES': 'document-question-answering',
 'MODEL_FOR_IMAGE_TO_IMAGE_MAPPING_NAMES': 'image-to-image',
 'MODEL_FOR_TIME_SERIES_PREDICTION_MAPPING_NAMES': 'time-series-forecasting',
}
tasks = collections.defaultdict(set)
for t, m in d['tables'].items():
    if t not in TASK:
        continue
    for mt, classes in m.items():
        for c in classes:
            tasks[c].add(TASK[t])
uniq = {c: next(iter(ts)) for c, ts in tasks.items() if len(ts) == 1}
amb = {c: sorted(ts) for c, ts in tasks.items() if len(ts) > 1}
print(len(uniq), 'unique;', len(amb), 'ambiguous', file=sys.stderr)
for c, ts in sorted(amb.items())[:40]:
    print('  AMB', c, ts, file=sys.stderr)
json.dump({'transformers': d['transformers'], 'tables': sorted(TASK.items()), 'unique': dict(sorted(uniq.items())), 'ambiguous': amb}, sys.stdout, indent=0)
