#!/bin/sh
set -eu
python3 tests/test_simulations.py
python3 tests/scheduling_batch.py
