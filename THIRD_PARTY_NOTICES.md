# Third-party notices

AudioBud is MIT-licensed (see [LICENSE](LICENSE)). It bundles an English word
list used by correction capture and redistributes a small set of Windows runtime
libraries app-locally. Those third-party components remain under their own
licenses, reproduced below.

The bundled Windows runtime set is defined by the packaging path, not by this document:
`scripts/bundle-runtime-dlls.mjs` (Tauri `beforeBundleCommand`) stages the VC++
runtime and the Vulkan loader, the WiX/MSI bundler harvests the executable's
sibling DLLs automatically, and `src-tauri/nsis/installer.nsi` copies each one
explicitly for parity. If that set changes, update this file to match.

## SCOWL English word list

File: `src-tauri/src/audio_toolkit/english_words_en.txt`

This lowercase word and contraction list is derived from the American English
dictionary distributed by Debian's `wamerican` package, which is generated from
SCOWL (Spell Checker Oriented Word Lists): https://wordlist.aspell.net/
It is a modified subset: Capitalized entries, single-character entries, entries
longer than 30 characters, and entries containing characters other than
lowercase letters or straight apostrophes were removed.

The collective work is Copyright 2000-2011 by Kevin Atkinson as well as any of
the copyrights mentioned in the SCOWL documentation.

Copyright 2000-2011 by Kevin Atkinson

Permission to use, copy, modify, distribute and sell these word lists, the
associated scripts, the output created from the scripts, and its documentation
for any purpose is hereby granted without fee, provided that the above copyright
notice appears in all copies and that both that copyright notice and this
permission notice appear in supporting documentation. Kevin Atkinson makes no
representations about the suitability of this array for any purpose. It is
provided "as is" without express or implied warranty.

The SCOWL components used by `wamerican` also incorporate material governed by
the following WordNet and Ispell notices.

### WordNet 1.6

This software and database is being provided to you, the LICENSEE, by Princeton
University under the following license. By obtaining, using and/or copying this
software and database, you agree that you have read, understood, and will comply
with these terms and conditions.

Permission to use, copy, modify and distribute this software and database and
its documentation for any purpose and without fee or royalty is hereby granted,
provided that you agree to comply with the following copyright notice and
statements, including the disclaimer, and that the same appear on ALL copies of
the software, database and documentation, including modifications that you make
for internal use or for distribution.

WordNet 1.6 Copyright 1997 by Princeton University. All rights reserved.

THIS SOFTWARE AND DATABASE IS PROVIDED "AS IS" AND PRINCETON UNIVERSITY MAKES
NO REPRESENTATIONS OR WARRANTIES, EXPRESS OR IMPLIED. BY WAY OF EXAMPLE, BUT NOT
LIMITATION, PRINCETON UNIVERSITY MAKES NO REPRESENTATIONS OR WARRANTIES OF
MERCHANTABILITY OR FITNESS FOR ANY PARTICULAR PURPOSE OR THAT THE USE OF THE
LICENSED SOFTWARE, DATABASE OR DOCUMENTATION WILL NOT INFRINGE ANY THIRD PARTY
PATENTS, COPYRIGHTS, TRADEMARKS OR OTHER RIGHTS.

The name of Princeton University or Princeton may not be used in advertising or
publicity pertaining to distribution of the software and/or database. Title to
copyright in this software, database and any associated documentation shall at
all times remain with Princeton University and LICENSEE agrees to preserve same.

### Ispell

```text
Copyright 1993, Geoff Kuenning, Granada Hills, CA. All rights reserved.

Redistribution and use in source and binary forms, with or without modification,
are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.
2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.
3. All modifications to the source code must be clearly marked as such. Binary
   redistributions based on modified source code must be clearly marked as
   modified versions in the documentation and/or other materials provided with
   the distribution.
(Clause 4 was removed with permission from Geoff Kuenning.)

5. The name of Geoff Kuenning may not be used to endorse or promote products
   derived from this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY GEOFF KUENNING AND CONTRIBUTORS "AS IS" AND ANY
EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED
WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL GEOFF KUENNING OR CONTRIBUTORS BE LIABLE FOR ANY
DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES
(INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES;
LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON
ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

## Microsoft Visual C++ runtime

Files: `msvcp140.dll`, `msvcp140_1.dll`, `vcruntime140.dll`, `vcruntime140_1.dll`

These are the Microsoft Visual C++ 2015-2022 Redistributable runtime libraries
(the "140" / v14.x shared toolset). They are copied from the build host's
`System32`, which holds the installed runtime. Microsoft permits app-local
deployment of these files: they appear on the Visual Studio "Distributable Code"
redistributable list, and the CRT is documented as supporting local
(side-by-side, next-to-the-exe) deployment.

They are redistributed under the Microsoft Software License Terms for the
applicable Visual Studio release, which govern the Distributable Code. See:

- Redistributing Visual C++ files:
  https://learn.microsoft.com/cpp/windows/redistributing-visual-cpp-files
- Local deployment of the Visual C++ runtime:
  https://learn.microsoft.com/cpp/windows/deployment-in-visual-cpp
- Microsoft Visual C++ Redistributable latest supported downloads:
  https://learn.microsoft.com/cpp/windows/latest-supported-vc-redist

Copyright (c) Microsoft Corporation. All rights reserved.

## Vulkan loader (`vulkan-1.dll`)

The Vulkan installable client driver loader. The loader ships with GPU drivers
rather than the Vulkan SDK, so CI fetches the LunarG VulkanRT runtime components
and bundles the extracted x64 loader; a driver-equipped build host can instead
use the `System32` copy. Source: https://github.com/KhronosGroup/Vulkan-Loader

Licensed under the Apache License, Version 2.0.

Copyright (c) The Khronos Group Inc.
Copyright (c) Valve Corporation
Copyright (c) LunarG, Inc.
Copyright (c) Google, Inc.

```
                                 Apache License
                           Version 2.0, January 2004
                        http://www.apache.org/licenses/

   TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

   1. Definitions.

      "License" shall mean the terms and conditions for use, reproduction,
      and distribution as defined by Sections 1 through 9 of this document.

      "Licensor" shall mean the copyright owner or entity authorized by
      the copyright owner that is granting the License.

      "Legal Entity" shall mean the union of the acting entity and all
      other entities that control, are controlled by, or are under common
      control with that entity. For the purposes of this definition,
      "control" means (i) the power, direct or indirect, to cause the
      direction or management of such entity, whether by contract or
      otherwise, or (ii) ownership of fifty percent (50%) or more of the
      outstanding shares, or (iii) beneficial ownership of such entity.

      "You" (or "Your") shall mean an individual or Legal Entity
      exercising permissions granted by this License.

      "Source" form shall mean the preferred form for making modifications,
      including but not limited to software source code, documentation
      source, and configuration files.

      "Object" form shall mean any form resulting from mechanical
      transformation or translation of a Source form, including but
      not limited to compiled object code, generated documentation,
      and conversions to other media types.

      "Work" shall mean the work of authorship, whether in Source or
      Object form, made available under the License, as indicated by a
      copyright notice that is included in or attached to the work
      (an example is provided in the Appendix below).

      "Derivative Works" shall mean any work, whether in Source or Object
      form, that is based on (or derived from) the Work and for which the
      editorial revisions, annotations, elaborations, or other modifications
      represent, as a whole, an original work of authorship. For the purposes
      of this License, Derivative Works shall not include works that remain
      separable from, or merely link (or bind by name) to the interfaces of,
      the Work and Derivative Works thereof.

      "Contribution" shall mean any work of authorship, including
      the original version of the Work and any modifications or additions
      to that Work or Derivative Works thereof, that is intentionally
      submitted to Licensor for inclusion in the Work by the copyright owner
      or by an individual or Legal Entity authorized to submit on behalf of
      the copyright owner. For the purposes of this definition, "submitted"
      means any form of electronic, verbal, or written communication sent
      to the Licensor or its representatives, including but not limited to
      communication on electronic mailing lists, source code control systems,
      and issue tracking systems that are managed by, or on behalf of, the
      Licensor for the purpose of discussing and improving the Work, but
      excluding communication that is conspicuously marked or otherwise
      designated in writing by the copyright owner as "Not a Contribution."

      "Contributor" shall mean Licensor and any individual or Legal Entity
      on behalf of whom a Contribution has been received by Licensor and
      subsequently incorporated within the Work.

   2. Grant of Copyright License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      copyright license to reproduce, prepare Derivative Works of,
      publicly display, publicly perform, sublicense, and distribute the
      Work and such Derivative Works in Source or Object form.

   3. Grant of Patent License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      (except as stated in this section) patent license to make, have made,
      use, offer to sell, sell, import, and otherwise transfer the Work,
      where such license applies only to those patent claims licensable
      by such Contributor that are necessarily infringed by their
      Contribution(s) alone or by combination of their Contribution(s)
      with the Work to which such Contribution(s) was submitted. If You
      institute patent litigation against any entity (including a
      cross-claim or counterclaim in a lawsuit) alleging that the Work
      or a Contribution incorporated within the Work constitutes direct
      or contributory patent infringement, then any patent licenses
      granted to You under this License for that Work shall terminate
      as of the date such litigation is filed.

   4. Redistribution. You may reproduce and distribute copies of the
      Work or Derivative Works thereof in any medium, with or without
      modifications, and in Source or Object form, provided that You
      meet the following conditions:

      (a) You must give any other recipients of the Work or
          Derivative Works a copy of this License; and

      (b) You must cause any modified files to carry prominent notices
          stating that You changed the files; and

      (c) You must retain, in the Source form of any Derivative Works
          that You distribute, all copyright, patent, trademark, and
          attribution notices from the Source form of the Work,
          excluding those notices that do not pertain to any part of
          the Derivative Works; and

      (d) If the Work includes a "NOTICE" text file as part of its
          distribution, then any Derivative Works that You distribute must
          include a readable copy of the attribution notices contained
          within such NOTICE file, excluding those notices that do not
          pertain to any part of the Derivative Works, in at least one
          of the following places: within a NOTICE text file distributed
          as part of the Derivative Works; within the Source form or
          documentation, if provided along with the Derivative Works; or,
          within a display generated by the Derivative Works, if and
          wherever such third-party notices normally appear. The contents
          of the NOTICE file are for informational purposes only and
          do not modify the License. You may add Your own attribution
          notices within Derivative Works that You distribute, alongside
          or as an addendum to the NOTICE text from the Work, provided
          that such additional attribution notices cannot be construed
          as modifying the License.

      You may add Your own copyright statement to Your modifications and
      may provide additional or different license terms and conditions
      for use, reproduction, or distribution of Your modifications, or
      for any such Derivative Works as a whole, provided Your use,
      reproduction, and distribution of the Work otherwise complies with
      the conditions stated in this License.

   5. Submission of Contributions. Unless You explicitly state otherwise,
      any Contribution intentionally submitted for inclusion in the Work
      by You to the Licensor shall be under the terms and conditions of
      this License, without any additional terms or conditions.
      Notwithstanding the above, nothing herein shall supersede or modify
      the terms of any separate license agreement you may have executed
      with Licensor regarding such Contributions.

   6. Trademarks. This License does not grant permission to use the trade
      names, trademarks, service marks, or product names of the Licensor,
      except as required for reasonable and customary use in describing the
      origin of the Work and reproducing the content of the NOTICE file.

   7. Disclaimer of Warranty. Unless required by applicable law or
      agreed to in writing, Licensor provides the Work (and each
      Contributor provides its Contributions) on an "AS IS" BASIS,
      WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
      implied, including, without limitation, any warranties or conditions
      of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
      PARTICULAR PURPOSE. You are solely responsible for determining the
      appropriateness of using or redistributing the Work and assume any
      risks associated with Your exercise of permissions under this License.

   8. Limitation of Liability. In no event and under no legal theory,
      whether in tort (including negligence), contract, or otherwise,
      unless required by applicable law (such as deliberate and grossly
      negligent acts) or agreed to in writing, shall any Contributor be
      liable to You for damages, including any direct, indirect, special,
      incidental, or consequential damages of any character arising as a
      result of this License or out of the use or inability to use the
      Work (including but not limited to damages for loss of goodwill,
      work stoppage, computer failure or malfunction, or any and all
      other commercial damages or losses), even if such Contributor
      has been advised of the possibility of such damages.

   9. Accepting Warranty or Additional Liability. While redistributing
      the Work or Derivative Works thereof, You may choose to offer,
      and charge a fee for, acceptance of support, warranty, indemnity,
      or other liability obligations and/or rights consistent with this
      License. However, in accepting such obligations, You may act only
      on Your own behalf and on Your sole responsibility, not on behalf
      of any other Contributor, and only if You agree to indemnify,
      defend, and hold each Contributor harmless for any liability
      incurred by, or claims asserted against, such Contributor by reason
      of your accepting any such warranty or additional liability.

   END OF TERMS AND CONDITIONS
```

## Microsoft DirectML (`DirectML.dll`)

The DirectML runtime, delivered as a build product of the ONNX Runtime DirectML
execution provider (the `ort-directml` build) that AudioBud runs on. It is the
`DirectML.dll` from Microsoft's `Microsoft.AI.DirectML` redistributable package;
the MSI harvests it as a sibling of the executable and the NSIS template copies
it explicitly, so both installers ship the exact DirectML version the app was
built against rather than relying on an OS in-box copy.

`DirectML.dll` is a Microsoft redistributable binary, not open-source software.
It is redistributed under the Microsoft Software License Terms attached to the
`Microsoft.AI.DirectML` package, which cover redistributing the runtime with an
application. The MIT license on the `microsoft/DirectML` GitHub repository
applies to that repository's samples, headers, and documentation, not to the
redistributable `DirectML.dll` binary, so it is not reproduced here. See the
package and its license terms:

- https://www.nuget.org/packages/Microsoft.AI.DirectML (License section)
- https://learn.microsoft.com/windows/ai/directml/dml

Copyright (c) Microsoft Corporation. All rights reserved.
