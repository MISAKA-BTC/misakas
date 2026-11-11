#!/bin/sh
set -eu
cd /tmp/t12-tokenizer-native-098da5e04
export TKR_MEASUREMENT_LOGICAL_RAM_BYTES=4294967296
export TKR_MEASUREMENT_LOGICAL_DB_BYTES=4294967296
python3 measure_linux.py ./misaka-tokenizer-registration-probe ./candidate-fixtures ./native-candidate-logical4g --source-commit 098da5e04136e4e6c8050c0851bde81974990783 --mode native-candidate --repetitions 5
python3 measure_linux.py ./misaka-tokenizer-registration-probe /tmp/t12-tokenizer-native-4d0aee958 ./native-multipass-logical4g --source-commit 098da5e04136e4e6c8050c0851bde81974990783 --mode native-multipass --repetitions 5
python3 measure_linux.py ./misaka-tokenizer-registration-probe ./candidate-fixtures ./scanner-m780 --source-commit 098da5e04136e4e6c8050c0851bde81974990783 --mode scanner --source-count 780 --repetitions 5
