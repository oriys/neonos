#!/bin/sh
# CI checkouts are shallow; materialize the frozen public course baseline if needed.
set -eu
lab_base=$(python3 -c 'import sys; sys.path.insert(0,"tools"); from lab_catalog import BASE; print(BASE)')
if ! git cat-file -e "${lab_base}^{commit}" 2>/dev/null; then
    git fetch --no-tags --depth=1 origin "$lab_base"
fi
python3 tests/test_lab_runner.py
