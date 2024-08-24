# Tools

The scripts in this folder help with commands with long arguments

* generate_llvm_coverage.sh **(RECOMMENDED)**

Generate coverage using the llvm coverage tool. The output files are generated to reports/llvm/html/index.html, reports/llvm/lcov.info & reports/llvm/lcov/index.html

``` bash
$ sh tools/generate_llvm_coverage.sh 
... 
Writing directory view page.
Overall coverage rate:
  lines......: 78.1% (890 of 1140 lines)
  functions..: 55.1% (201 of 365 functions)
llvm-cov report generated to reports/llvm/html/index.html 
lcov report generated to: reports/llvm/lcov/index.html 
```

* generate_tarpaulin_coverage.sh

This script generates coverage reports using the tarpaulin interface _and_ lcov. The output files are generated to reports/tarpaulin-report.html, reports/lcov.info & reports/lcov/index.html.

execute the script from the repo root:
``` bash
$ sh tools/generate_tarpaulin_coverage.sh
...
Overall coverage rate:
  lines......: 75.1% (431 of 574 lines)
  functions..: 82.3% (51 of 62 functions)
tarpaulin report generated to reports/tarpaulin-report.html 
lcov report generated to: reports/lcov/index.html 
```

