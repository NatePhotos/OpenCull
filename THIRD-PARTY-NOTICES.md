# Third-party notices

OpenCull is licensed under the GNU General Public License, version 3 or
later (see `LICENSE`). It is built on, and its downloads include, the work of
others. Each is used under its own licence, all of which allow this.

## Included in every download

| What | What it does here | Licence | Copyright |
|---|---|---|---|
| [ONNX Runtime](https://github.com/microsoft/onnxruntime) | Runs the two face models | MIT | © Microsoft Corporation |
| [YuNet](https://github.com/opencv/opencv_zoo/tree/main/models/face_detection_yunet) (`src-tauri/models/finder.onnx`) | Finds faces. The model file is re-saved to accept a picture of any size; nothing else about it is changed | MIT | © 2020 Shiqi Yu |
| [libjpeg-turbo](https://github.com/libjpeg-turbo/libjpeg-turbo) | Decodes and writes previews | IJG licence, Modified BSD (3-clause) and zlib | © the libjpeg-turbo Project and the Independent JPEG Group. This software is based in part on the work of the Independent JPEG Group. |
| [Tauri](https://github.com/tauri-apps/tauri) and its plugins | The window, and the bridge between the interface and the rest | MIT or Apache-2.0 | © the Tauri Programme within The Commons Conservancy |
| [React](https://github.com/facebook/react) | The interface | MIT | © Meta Platforms, Inc. and affiliates |
| [Radix UI](https://github.com/radix-ui/primitives) | Menus, dialogs, switches, tabs | MIT | © WorkOS |
| [shadcn/ui](https://github.com/shadcn-ui/ui) (`src/styles/shadcn-tailwind.css`, and the components in `src/components/ui`) | The look of menus, dialogs and switches | MIT | © shadcn |
| [Lucide](https://github.com/lucide-icons/lucide) | Icons | ISC | © Lucide Contributors |
| [Tailwind CSS](https://github.com/tailwindlabs/tailwindcss) | Styling | MIT | © Tailwind Labs, Inc. |
| [Zustand](https://github.com/pmndrs/zustand) | Interface state | MIT | © Paul Henschel |
| [TanStack Virtual](https://github.com/TanStack/virtual) | The grid view | MIT | © Tanner Linsley |

The Rust crates and npm packages these bring with them, and the smaller ones
used directly, are listed with their versions in `src-tauri/Cargo.lock` and
`package-lock.json`; each carries its licence in its own package. All are
under MIT, Apache-2.0, BSD, ISC, zlib or similar permissive licences.

## The face judge

`src-tauri/models/judge.onnx` is this project's own: a small network that
judges a face — eyes, gaze, sharpness, subject or background. It is released
with the rest of the project under the same licence.

## The model files, exactly

So a copy can be checked against the one released, the SHA-256 of each:

| File | SHA-256 |
|---|---|
| `src-tauri/models/finder.onnx` | `c9d7f075034be96ce7ae4791a1c8b23b06180250885af870b8ee461e14c506c5` |
| `src-tauri/models/judge.onnx` | `4a4b5edfe02c6502995d2b68d7ca3b240a90d22ed9d0e2b9eb28d9e49c4c0118` |

A test (`ai::store::tests`) fails if either file changes without this table
being updated.

## ONNX Runtime at build time

The `ort` crate, at version 2.0.0-rc.13 (a release candidate, pinned
exactly), downloads a prebuilt ONNX Runtime library from the `ort` project
while the app builds. So a build needs the network, and that library comes
from outside crates.io. To build from a copy of ONNX Runtime of your own
instead, set `ORT_LIB_LOCATION` to the folder that holds it; see the `ort`
documentation.

## The face finder's training data

YuNet's authors report it on the WIDER FACE benchmark. That dataset is made
available by its authors for research; the model itself is released by its
authors under the MIT licence above.
