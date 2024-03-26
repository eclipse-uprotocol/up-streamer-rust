# Contributing to Eclipse uProtocol

Thanks for your interest in this project. Contributions are welcome!

Please refer to the main [CONTRIBUTING](https://github.com/eclipse-uprotocol/.github/blob/main/CONTRIBUTING.adoc) resource for Eclipse uProtocol to get most of the general overview.

The following is from [`up-rust`](https://github.com/eclipse-uprotocol/up-rust) project and is specific to Rust development for uProtocol.

## Setting up a development environment

You can use any development environment you like to contribute to  `up-streamer-rust`. However, it is mandatory to use the Rust linter ('[clippy](<https://github.com/rust-lang/rust-clippy>)') for any pull requests you do.
To set up VSCode to run clippy per default every time you save your code, have a look here: [How to use Clippy in VS Code with rust-analyzer?](https://users.rust-lang.org/t/how-to-use-clippy-in-vs-code-with-rust-analyzer/41881)

Similarly, the project requests that markdown is formatted and linted properly - to help with this, it is recommended to use [markdown linters](https://marketplace.visualstudio.com/items?itemName=DavidAnson.vscode-markdownlint).

During development, before submitting a PR, you can use `./tools/fmt_clippy_doc.sh` to run these checks on the workspace.

There also exists a helper script in ./tools to generate test results and test code coverage reports. These reports are placed in the `./target/tarpaulin` directory. If you use VSCode with the [Coverage Gutters](https://marketplace.visualstudio.com/items?itemName=ryanluker.vscode-coverage-gutters) extension, you can enable display of code coverage information with these settings:

``` json
  "coverage-gutters.coverageBaseDir": "**",
  "coverage-gutters.coverageFileNames": [
    "target/tarpaulin/lcov.info",
  ],
```

## DevContainer

All of these prerequisites are made available as a VSCode devcontainer, configured at the usual place (`.devcontainer`).

## Contact

Contact the project developers via the project's "dev" list.

<https://accounts.eclipse.org/mailing-list/uprotocol-dev>
