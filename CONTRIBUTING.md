# Contribution

We welcome contributions to this project. Please feel free to open GitHub issues, pull requests, or comments.

## Coding Style

[`rustfmt`] and [`clippy`] is used to enforce coding style. Before pushing a commit, run `cargo fmt --all` to format your code and make sure [`clippy`] warnings are fixed.

[`rustfmt`]: https://github.com/rust-lang/rustfmt
[`clippy`]: https://github.com/rust-lang/clippy

## Testing

All changes submitted to this repository are run through GitHub Actions and the workflow defined into the [`ci.yml`] file. If your change does not pass the tests described, it is unlikely to be merged.

[`ci.yml`]: https://github.com/notgull/theo/blob/main/.github/workflows/ci.yml

## Contributor License Agreement

When opening a pull request, you will be asked to sign a [Contributor License Agreement (CLA)](https://cla-assistant.io/notgull/theo). This is a legal document that confirms you are granting us permission to use your contribution. You only need to sign the CLA once.
