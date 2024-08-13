#!/bin/bash

################################################################################
# Copyright (c) 2024 Contributors to the Eclipse Foundation
#
# See the NOTICE file(s) distributed with this work for additional
# information regarding copyright ownership.
#
# This program and the accompanying materials are made available under the
# terms of the Apache License Version 2.0 which is available at
# https://www.apache.org/licenses/LICENSE-2.0
#
# SPDX-License-Identifier: Apache-2.0
################################################################################

BLUE='\033[0;34m'
NC='\033[0m'
HAS_LLVMCOV=$(cargo install --list | grep cargo-llvm-cov)
COVERAGE_OUT=reports/llvm

if [ -z "$HAS_LLVMCOV" ]; then
    echo "cargo-llvm-cov not found, please install it with 'cargo install cargo-llvm-cov'"
    exit 1
fi

mkdir -p $COVERAGE_OUT

# generate coverage using llvm
# RUST_LOG ensures the debug macros are not flagged by the coverage output
RUST_LOG=trace cargo llvm-cov --html --output-dir $COVERAGE_OUT --ignore-filename-regex 'utils/.*' 2>&1 | grep -Ev 'TRACE|DEBUG|WARN'

# generate lcov coverage 
RUST_LOG=trace cargo llvm-cov --lcov --output-path $COVERAGE_OUT/lcov.info --ignore-filename-regex 'utils/.*' 2>&1 | grep -Ev 'TRACE|DEBUG|WARN' 
genhtml -o $COVERAGE_OUT/lcov/ --show-details --highlight --ignore-errors source --legend $COVERAGE_OUT/lcov.info

printf "${BLUE}"
printf "llvm-cov report generated to \e]8;;file://%s\a%s\e]8;;\a \n" "$PWD/$COVERAGE_OUT/html/index.html" "$COVERAGE_OUT/html/index.html"
printf 'lcov report generated to: \e]8;;file://%s\a%s\e]8;;\a \n' "$PWD/$COVERAGE_OUT/lcov/index.html" "$COVERAGE_OUT/lcov/index.html"
printf "${NC}"
