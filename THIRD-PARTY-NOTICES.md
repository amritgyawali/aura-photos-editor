# Third-party notices

## YuNet portrait detection weights

AURA bundles OpenCV Zoo's `face_detection_yunet_2023mar.onnx`, copyright
2020 Shiqi Yu, under the MIT license. The complete notice and permission terms
are in `assets/models/yunet/LICENSE`; model provenance and pinned hashes are in
`assets/models/yunet/MODEL-CARD.md`. AURA's portrait planner is independently authored.

> MIT License
>
> Copyright (c) 2020 Shiqi Yu <shiqi.yu@gmail.com>
>
> Permission is hereby granted, free of charge, to any person obtaining a copy
> of this software and associated documentation files (the "Software"), to deal
> in the Software without restriction, including without limitation the rights
> to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
> copies of the Software, and to permit persons to whom the Software is
> furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in all
> copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
> AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
> OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
> SOFTWARE.

AURA links third-party Rust crates. The licence policy is `deny.toml`, checked by
`cargo deny check` in CI lane 4: MIT, Apache-2.0 (with or without the LLVM
exception), BSD-2-Clause, BSD-3-Clause, ISC, Unicode-3.0, Zlib and MPL-2.0 are
allowed for every dependency. Anything else needs a scoped exception in that file
and an entry here.

## Independent JPEG Group (IJG) - `jpeg-encoder`

`jpeg-encoder` is distributed under `(MIT OR Apache-2.0) AND IJG`. The IJG half
carries one obligation, and this is it:

> This software is based in part on the work of the Independent JPEG Group.

The crate is used by `aura-raw` to write the JPEG previews and the exported
proxies. No IJG code is modified, and no claim is made that AURA is the original
software.

## OpenCV Haar cascades - `crates/aura-portrait/data/`

`aura-portrait` embeds three trained Haar cascades from OpenCV (`haarcascade_frontalface_alt2.xml`,
`haarcascade_profileface.xml` and `haarcascade_eye.xml`, from the `opencv/opencv` repository's
`data/haarcascades` directory), converted to a line format by
`crates/aura-portrait/data/convert_opencv_cascade.py`. The converted files carry the licence text
verbatim in their header comments, and it is reproduced here:

> Intel License Agreement
> For Open Source Computer Vision Library
>
> Copyright (C) 2000, Intel Corporation, all rights reserved.
> Third party copyrights are property of their respective owners.
>
> Redistribution and use in source and binary forms, with or without modification,
> are permitted provided that the following conditions are met:
>
> * Redistribution's of source code must retain the above copyright notice,
> this list of conditions and the following disclaimer.
>
> * Redistribution's in binary form must reproduce the above copyright notice,
> this list of conditions and the following disclaimer in the documentation
> and/or other materials provided with the distribution.
>
> * The name of Intel Corporation may not be used to endorse or promote products
> derived from this software without specific prior written permission.
>
> This software is provided by the copyright holders and contributors "as is" and
> any express or implied warranties, including, but not limited to, the implied
> warranties of merchantability and fitness for a particular purpose are disclaimed.
> In no event shall the Intel Corporation or contributors be liable for any direct,
> indirect, incidental, special, exemplary, or consequential damages
> (including, but not limited to, procurement of substitute goods or services;
> loss of use, data, or profits; or business interruption) however caused
> and on any theory of liability, whether in contract, strict liability,
> or tort (including negligence or otherwise) arising in any way out of
> the use of this software, even if advised of the possibility of such damage.

The frontal cascade was created by Rainer Lienhart, the profile cascade contributed by David Bradley
of Princeton University, and the eye cascade created by Shameem Hameed. AURA evaluates them with its own code (`crates/aura-portrait/src/cascade.rs`); no OpenCV
code is linked or copied.
