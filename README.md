# theo

A generic [`piet`] rendering context for all windowing and graphics backends.

Windowing frameworks like [`winit`] do not provide a way to draw into them by default. This decision is intentional; it allows the user to choose which graphics backend that they'd like to use, and also makes maintaining the windowing code much simpler. For games (what [`winit`] was originally designed for), usually a 3D rendering context like [`wgpu`] or [`glow`] would be used in this case. However, GUI applications will need a 2D vector graphics context.

[`piet`] is a 2D graphics abstraction that can be used with many different graphics backends. However, [`piet`]'s default implementation, [`piet-common`], is difficult to integrate with windowing systems. [`theo`] aims to bridge this gap by providing a generic [`piet`] rendering context that easily integrates with windowing systems.

[`piet`]: https://crates.io/crates/piet
[`piet-common`]: https://crates.io/crates/piet-common
[`winit`]: https://crates.io/crates/winit
[`wgpu`]: https://crates.io/crates/wgpu
[`glow`]: https://crates.io/crates/glow
[`theo`]: https://crates.io/crates/theo

## License

`theo` is free software: you can redistribute it and/or modify it under the terms of
either:

* GNU Lesser General Public License as published by the Free Software Foundation, either
version 3 of the License, or (at your option) any later version.
* Mozilla Public License as published by the Mozilla Foundation, version 2.
* The Patron License (https://github.com/notgull/theo/blob/main/LICENSE-PATRON.md) for sponsors and contributors, who can ignore the copyleft provisions of the above licenses or this project.

`theo` is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
See the GNU Lesser General Public License or the Mozilla Public License for more details.

You should have received a copy of the GNU Lesser General Public License and the Mozilla
Public License along with `theo`. If not, see <https://www.gnu.org/licenses/>.