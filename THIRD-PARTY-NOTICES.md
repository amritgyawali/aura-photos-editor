# Third-party notices

## MediaPipe Selfie Multiclass segmentation weights

AURA bundles a conversion of Google's MediaPipe Selfie Multiclass image segmenter
(`selfie_multiclass_256x256.tflite`), Copyright Google LLC, under the Apache License,
Version 2.0. The converted file is `assets/models/selfie_multiclass/selfie_multiclass_256x256.onnx`;
the weights are unchanged and only the tensor layout was converted, by
`ml/models/skin/convert_selfie_multiclass.py`. The license text is in
`assets/models/selfie_multiclass/LICENSE`; provenance and pinned hashes are in
`assets/models/selfie_multiclass/MODEL-CARD.md`. You may obtain a copy of the license at
http://www.apache.org/licenses/LICENSE-2.0. Distributed on an "AS IS" BASIS, WITHOUT
WARRANTIES OR CONDITIONS OF ANY KIND.

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
