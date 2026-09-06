#!/bin/sh
# Lesson 10 normal batch and independent expected kernel-fault sessions.
set -eu
sh tests/lesson-08.sh
python3 tests/user_batch.py
