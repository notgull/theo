# Contribution

We welcome contributions to this project. Please feel free to open GitHub issues, pull requests, or comments.

## Coding Style

[`rustfmt`] and [`clippy`] is used to enforce coding style. Before pushing a commit, run `cargo fmt --all` to format your code and make sure [`clippy`] warnings are fixed.

[`rustfmt`]: https://github.com/rust-lang/rustfmt
[`clippy`]: https://github.com/rust-lang/clippy

## Testing

All changes submitted to this repository are run through GitHub Actions and the workflow defined into the [`ci.yml`] file. If your change does not pass the tests described, it is unlikely to be merged.

[`ci.yml`]: https://github.com/notgull/theo/blob/main/.github/workflows/ci.yml

## DCO

As an alternative to a Contributor License Agreement, this project uses a [Developer Certificate of Origin (DCO)](./DCO.txt) to ensure that contributors own the copyright terms of their contributions. In order to assert that you agree to the terms of the DCO, you must add the following line to every commit:

```
Signed-off-by: Your Name <email>
```

This can be done automatically by appending the `-s` option to `git commit`.
