# Third-party notices

This file covers third-party material stored in the source tree and assets emitted by the
documentation build. Release binaries carry a separate `THIRD_PARTY_LICENSES.txt`, assembled
from the locked four-platform Cargo graph by `scripts/release-licenses.sh` without an added tool.

## Parser evidence

| Material | Pinned source | Terms and attribution |
| --- | --- | --- |
| `corpus/third_party/commonmark-0.31.2.spec.json` | CommonMark 0.31.2 | [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/), copyright 2014–2016 John MacFarlane. The JSON is extracted and normalized from the specification examples. |
| `corpus/third_party/gfm-0.29.spec.txt` | `github/cmark-gfm` tag `0.29.0.gfm.13` | [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/), copyright GitHub, Inc. and John MacFarlane. The specification is GitHub's extension of CommonMark. |
| `micromark-mdx-jsx-3.0.2.test.js` | `micromark-extension-mdx-jsx` `ad0a49c` | MIT, copyright 2020 Titus Wormer. |
| `micromark-mdx-expression-3.0.1.test.js` | `micromark-extension-mdx-expression` `2891b75` | MIT, copyright Titus Wormer. |
| `micromark-mdxjs-esm-3.0.0.test.js` | `micromark-extension-mdxjs-esm` `7cc9131` | MIT, copyright 2020 Titus Wormer. |
| `micromark-gfm-footnote-2.1.0.test.js` and `gfm-footnote-fixtures/` | `micromark-extension-gfm-footnote` `df527f5` | MIT, copyright 2021 Titus Wormer. The HTML files record GitHub's rendering of the paired fixture documents. |
| `micromark-gfm-strikethrough-2.1.0.test.js` | `micromark-extension-gfm-strikethrough` `a3a75cc` | MIT, copyright 2020 Titus Wormer. |

`corpus/parser-profile-corpus.json` is a composite generated from those inputs and Amiss's
extraction results. Source-derived material remains under its upstream terms; the selection,
measurements, and Amiss-authored fields remain under the project license.

## Documentation and fonts

The four Latin Modern WOFF2 files are byte-identical to `vincentdoerig/latex-css` commit
`2de5cc58d87b3a58413020f9f15bd8c261c29e13`. Latin Modern is copyright 2003–2021 B. Jackowski
and J.M. Nowacki and distributed under the [GUST Font License](docs/src/fonts/GUST-FONT-LICENSE.txt).
The upstream web packaging is MIT, copyright 2020 Vincent Dörig.

The site is built with mdBook 0.5.4, licensed under
[MPL-2.0](https://github.com/rust-lang/mdBook/blob/v0.5.4/LICENSE). Its generated assets retain
their inline notices, including Highlight.js 10.1.1 (BSD-3-Clause, copyright 2006–2020 Ivan
Sagalaev), elasticlunr 0.9.5 (MIT, copyright 2017 Oliver Nightingale and Wei Song), clipboard.js
2.0.4 (MIT, copyright Zeno Rocha), mark.js 8.11.1 (MIT, copyright 2014–2018 Julian Kühnel), and
Font Awesome Free 6.2.0 (icons CC BY 4.0, fonts SIL OFL 1.1, code MIT, copyright 2022 Fonticons,
Inc.).

## MIT license

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and
associated documentation files (the "Software"), to deal in the Software without restriction,
including without limitation the rights to use, copy, modify, merge, publish, distribute,
sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or
substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT
NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT
OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

## BSD 3-Clause license

Redistribution and use in source and binary forms, with or without modification, are permitted
provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this list of
   conditions and the following disclaimer.
2. Redistributions in binary form must reproduce the above copyright notice, this list of
   conditions and the following disclaimer in the documentation and/or other materials provided
   with the distribution.
3. Neither the name of the copyright holder nor the names of its contributors may be used to
   endorse or promote products derived from this software without specific prior written
   permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR
IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND
FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR
CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER
IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT
OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
