#!/bin/sh
# Called by the existing CI boot step; may also be run directly on a developer host.
set -eu
sh tests/labs.sh
sh tests/user.sh
sh tests/scheduling.sh
NEONOS_PROFILE=release python3 tests/user_batch.py
NEONOS_PROFILE=release python3 tests/scheduling_batch.py
cargo build
