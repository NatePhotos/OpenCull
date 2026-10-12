<a id="readme-top"></a>

<!-- PROJECT SHIELDS -->
[![Release][release-shield]][release-url]
[![Build][build-shield]][build-url]
[![Stargazers][stars-shield]][stars-url]
[![Issues][issues-shield]][issues-url]
[![GPL-3.0][license-shield]][license-url]
[![Instagram][instagram-shield]][instagram-url]

<!-- PROJECT LOGO -->
<br />
<div align="center">
  <a href="https://github.com/NatePhotos/OpenCull">
    <img src="docs/images/logo.png" alt="OpenCull" width="96" height="96">
  </a>

<h3 align="center">OpenCull</h3>

  <p align="center">
    Photo culling for photographers: fast, private and free.
    <br />
    For macOS, Windows and Linux.
    <br />
    <a href="docs/HOW-IT-WORKS.md"><strong>How it works »</strong></a>
    <br />
    <br />
    <a href="https://github.com/NatePhotos/OpenCull/releases">Download</a>
    &middot;
    <a href="https://github.com/NatePhotos/OpenCull/issues/new?labels=bug">Report Bug</a>
    &middot;
    <a href="https://github.com/NatePhotos/OpenCull/issues/new?labels=enhancement">Request Feature</a>
  </p>
</div>

<!-- TABLE OF CONTENTS -->
<details>
  <summary>Table of Contents</summary>
  <ol>
    <li>
      <a href="#about-the-project">About The Project</a>
      <ul>
        <li><a href="#built-with">Built With</a></li>
      </ul>
    </li>
    <li>
      <a href="#getting-started">Getting Started</a>
      <ul>
        <li><a href="#download">Download</a></li>
        <li><a href="#build-from-source">Build From Source</a></li>
      </ul>
    </li>
    <li><a href="#usage">Usage</a></li>
    <li><a href="#contributing">Contributing</a></li>
    <li><a href="#license">License</a></li>
    <li><a href="#contact">Contact</a></li>
    <li><a href="#acknowledgments">Acknowledgments</a></li>
  </ol>
</details>

<!-- ABOUT THE PROJECT -->
## About The Project

[![OpenCull, with a photo open: the camera's settings on the left, the photo in the middle, each face cut out large on the right, and the filmstrip below][product-screenshot]](docs/images/screenshot.jpg)

Culling is the first job after a shoot: going through every frame and
deciding what to keep. OpenCull is built to make that fast.

* **Opens a shoot at once.** A folder of RAWs opens in a fraction of a second,
  straight from the previews the camera already wrote into each file. No
  import, no catalogue, nothing to wait for.
* **RAW, JPEG or both.** Shoot both and each pair is culled as one photo.
  Or choose to see only the RAWs, or only the JPEGs.
* **Marks you can take anywhere.** Stars, flags and colour labels are written
  as standard sidecar files that Lightroom, Capture One and the rest read.
  Turn on **Write ratings into JPEGs** in Settings to also write a JPEG's
  stars and colour label into the file itself, where those programs look for
  them. It is off by default.
* **Looks at the faces for you.** It finds the faces in every photo and shows
  whose eyes are shut, who is looking away and how sharp each one is.
* **Compares a person with themselves.** It follows each person through a
  burst, or through a pose shot again, and ranks the frames, so the one with
  everyone's eyes open is the one it puts first.
* **Keeps your photos yours.** Everything runs on your own computer. Nothing
  is uploaded, and there is no account. The one thing it uses the internet
  for is asking whether a newer version of itself is out, which sends nothing
  about you or your photos and can be turned off in Settings.
* **Keeps itself up to date.** When a new version is out, OpenCull says so
  when it opens and installs it for you if you want it.
* **Start to finish.** Copy photos off a card, cull them, and hand the keepers
  to your editing program.

**Status: early.** It is used daily on a Mac, and has been tested and
confirmed working on Windows and Linux. If something goes wrong on your
system, reports are very welcome.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

### Built With

* [![Tauri][Tauri-badge]][Tauri-url]
* [![Rust][Rust-badge]][Rust-url]
* [![React][React-badge]][React-url]
* [![TypeScript][TypeScript-badge]][TypeScript-url]
* [![Tailwind CSS][Tailwind-badge]][Tailwind-url]
* [![ONNX Runtime][ONNX-badge]][ONNX-url]

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- GETTING STARTED -->
## Getting Started

### Download

Installers are on the [releases page][release-url].

| System | File |
|---|---|
| macOS, Apple silicon (M1 and later) | `.dmg` |
| Windows 10 and 11 | `-setup.exe` |
| Linux (Ubuntu 24.04, Fedora 40, Debian 13 or newer) | `.AppImage`, `.deb` or `.rpm` |

They are not signed with a paid developer certificate, so each system asks
before running them the first time:

* **macOS** — open the `.dmg` and drag OpenCull to Applications. The first
  time you open it macOS will refuse. Go to System Settings → Privacy &
  Security, scroll down, and choose **Open Anyway**.
* **Windows** — SmartScreen shows "Windows protected your PC". Choose **More
  info**, then **Run anyway**.
* **Linux** — for the AppImage, make it executable (`chmod +x`) and run it.

That is the only time you need this page. From then on OpenCull offers each
new version itself, and installing it keeps your settings. If an update
cannot be installed where the app lives, it says so and brings you back here.

### Build From Source

You need [Rust](https://rustup.rs) and [Node](https://nodejs.org), and one
thing for the JPEG library: `brew install jpeg-turbo` on macOS, or CMake
(plus NASM on Intel and AMD machines) on Windows and Linux. Windows and Linux
also need the system packages on
[Tauri's list](https://tauri.app/start/prerequisites/).

1. Clone the repo
   ```sh
   git clone https://github.com/NatePhotos/OpenCull.git
   cd OpenCull
   ```
2. Install the packages
   ```sh
   npm install
   ```
3. Run it
   ```sh
   npm run tauri dev
   ```
4. Or build the app itself
   ```sh
   npm run tauri build
   ```

More on building, including how to build without the JPEG library, is in
[How it works](docs/HOW-IT-WORKS.md#building-the-fast-jpeg-library).

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- USAGE EXAMPLES -->
## Usage

Open a folder, or choose **Ingest from Card** to copy a card first. Then go
through the shoot from the keyboard:

| Key | What it does |
|---|---|
| `←` `→` | Previous and next photo |
| `↑` `↓` | Previous and next group (a burst, or a pose shot again) |
| `P` `X` `U` | Pick, reject, clear the flag |
| `1`–`5`, `0` | Star rating, and none |
| `6` `7` `8` `9` `N` | Red, yellow, green, blue and purple labels |
| `Space` | Zoom in to the middle of the photo, and back out |
| `Z` | Zoom in to where you last looked, and back out |
| `G` | Grid view |
| `.` | Face panel |
| `F` `H` `I` | Filmstrip, histogram and EXIF, info overlay |
| `Caps Lock` | Speed cull: move on to the next photo after each mark |
| `⌘E` / `Ctrl+E` | Ship: open the keepers in your editor, or copy or move them |
| `⌘Z` / `Ctrl+Z` | Undo |

Every shortcut can be changed in Settings.

<div align="center">
  <img src="docs/images/shortcuts.png" alt="The Keyboard Shortcuts window, listing every shortcut and the key it is on" width="520">
</div>

The face panel on the right shows each face in the photo, cut out large
enough to judge. The top square on each is its eyes (green for open, through
to red for shut); the bottom one is its sharpness out of ten. Point at them
for the words. The coloured bar under each thumbnail is the photo's rank
among the frames it was shot with, with **1** the one to look at first.

Drag the edge of either side panel, or the top of the filmstrip, to make it
larger or smaller. The sizes are remembered.

_For how any of this works, and how well, see
[How it works](docs/HOW-IT-WORKS.md)._

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- CONTRIBUTING -->
## Contributing

Contributions are welcome, most of all from anyone who can run OpenCull on
Windows or Linux and say what happened.

If you have a suggestion, open an issue with the tag "enhancement", or fork
the repo and open a pull request:

1. Fork the Project
2. Create your Feature Branch (`git checkout -b feature/AmazingFeature`)
3. Commit your Changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the Branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

`npm test` runs the unit tests. The app can also run itself against a folder
of your own photos with no window on screen and without touching them; see
[Checking it works](docs/HOW-IT-WORKS.md#checking-it-works).

Please do not attach other people's photographs to an issue.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- LICENSE -->
## License

Copyright © 2026 The OpenCull authors.

Distributed under the GNU General Public License, version 3 or (at your
option) any later version. It comes with no warranty. See [`LICENSE`](LICENSE)
for the full text, and [`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md) for
what OpenCull is built on and the licences those come under.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- CONTACT -->
## Contact

NatePhotos - [@natephotos.ny](https://www.instagram.com/natephotos.ny) - natephotos.ny@gmail.com

Project Link: [https://github.com/NatePhotos/OpenCull](https://github.com/NatePhotos/OpenCull)

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- ACKNOWLEDGMENTS -->
## Acknowledgments

* [Tauri](https://tauri.app)
* [ONNX Runtime](https://onnxruntime.ai) and the [ort](https://ort.pyke.io) crate
* [YuNet](https://github.com/opencv/opencv_zoo/tree/main/models/face_detection_yunet), the face finder
* [libjpeg-turbo](https://libjpeg-turbo.org)
* [shadcn/ui](https://ui.shadcn.com), [Radix UI](https://www.radix-ui.com) and [Lucide](https://lucide.dev)
* [Best-README-Template](https://github.com/othneildrew/Best-README-Template)
* [Shields.io](https://shields.io)

<p align="right">(<a href="#readme-top">back to top</a>)</p>

<!-- MARKDOWN LINKS & IMAGES -->
[product-screenshot]: docs/images/screenshot.jpg
[release-shield]: https://img.shields.io/github/v/release/NatePhotos/OpenCull?include_prereleases&style=for-the-badge
[release-url]: https://github.com/NatePhotos/OpenCull/releases
[build-shield]: https://img.shields.io/github/actions/workflow/status/NatePhotos/OpenCull/build.yml?branch=main&style=for-the-badge
[build-url]: https://github.com/NatePhotos/OpenCull/actions/workflows/build.yml
[stars-shield]: https://img.shields.io/github/stars/NatePhotos/OpenCull.svg?style=for-the-badge
[stars-url]: https://github.com/NatePhotos/OpenCull/stargazers
[issues-shield]: https://img.shields.io/github/issues/NatePhotos/OpenCull.svg?style=for-the-badge
[issues-url]: https://github.com/NatePhotos/OpenCull/issues
[license-shield]: https://img.shields.io/badge/license-GPL--3.0--or--later-blue?style=for-the-badge
[license-url]: https://github.com/NatePhotos/OpenCull/blob/main/LICENSE
[instagram-shield]: https://img.shields.io/badge/-Instagram-black.svg?style=for-the-badge&logo=instagram&colorB=555
[instagram-url]: https://www.instagram.com/natephotos.ny
[Tauri-badge]: https://img.shields.io/badge/Tauri-24C8D8?style=for-the-badge&logo=tauri&logoColor=white
[Tauri-url]: https://tauri.app
[Rust-badge]: https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white
[Rust-url]: https://www.rust-lang.org
[React-badge]: https://img.shields.io/badge/React-20232A?style=for-the-badge&logo=react&logoColor=61DAFB
[React-url]: https://react.dev
[TypeScript-badge]: https://img.shields.io/badge/TypeScript-3178C6?style=for-the-badge&logo=typescript&logoColor=white
[TypeScript-url]: https://www.typescriptlang.org
[Tailwind-badge]: https://img.shields.io/badge/Tailwind_CSS-06B6D4?style=for-the-badge&logo=tailwindcss&logoColor=white
[Tailwind-url]: https://tailwindcss.com
[ONNX-badge]: https://img.shields.io/badge/ONNX_Runtime-005CED?style=for-the-badge&logo=onnx&logoColor=white
[ONNX-url]: https://onnxruntime.ai
