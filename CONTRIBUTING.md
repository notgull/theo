# Contribution

We welcome contributions to this project. Please do not open issues or pull
requests on the GitHub mirror. Please open issues and pull requests on our
[`Git forge`](https://src.notgull.net/notgull/theo).

## Coding Style

[`rustfmt`] and [`clippy`] is used to enforce coding style. Before pushing a
commit, run `cargo fmt --all` to format your code and make sure [`clippy`]
warnings are fixed.

[`rustfmt`]: https://github.com/rust-lang/rustfmt
[`clippy`]: https://github.com/rust-lang/clippy

## Testing

All changes submitted to this repository are run through our Drone CI system.
Our CI scripts are stored [here](https://src.notgull.net/notgull/ci).

## DCO

As an alternative to a Contributor License Agreement, this project uses a
[Developer Certificate of Origin (DCO)](./DCO.txt) to ensure that contributors
own the copyright terms of their contributions. In order to assert that you
agree to the terms of the DCO, you must add the following line to every commit:

```plaintext
Signed-off-by: Your Name <email>
```

This can be done automatically by appending the `-s` option to `git commit`.
