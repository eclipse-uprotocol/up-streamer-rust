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
HAS_TARPAULIN=$(cargo install --list | grep cargo-tarpaulin)
HAS_GENHTML=$(which genhtml)
COVERAGE_OUT=reports

if [ -z "$HAS_TARPAULIN" ]; then
    echo "cargo-tarpaulin not found, please install it with 'cargo install cargo-tarpaulin'"
    exit 1
fi

if [ -z "$HAS_GENHTML" ]; then
    echo "genhtml not found, please install it with 'sudo apt install lcov'"
    exit 1
fi

# we want both html and lcov output formats
cargo tarpaulin -o lcov -o html --exclude-files 'utils/*' --output-dir $COVERAGE_OUT
# convert the lcov output to html
genhtml -o $COVERAGE_OUT/lcov/ --show-details --highlight --ignore-errors source --legend lcov.info

printf "${BLUE}"
printf "tarpaulin report generated to \e]8;;file://%s\a%s\e]8;;\a \n" "$PWD/$COVERAGE_OUT/tarpaulin-report.html" "$COVERAGE_OUT/tarpaulin-report.html"
printf 'lcov report generated to: \e]8;;file://%s\a%s\e]8;;\a \n' "$PWD/$COVERAGE_OUT/lcov/index.html" "$COVERAGE_OUT/lcov/index.html"
printf "${NC}"
