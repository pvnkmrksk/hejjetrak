# HejjéTrac v0.3.2

Linux X11/XWayland startup now prefers compatible OpenGL and automatically falls
back to Vulkan when OpenGL cannot present to the window. This fixes the
v0.3.1 startup failure that required `WGPU_BACKEND=vulkan` on an NVIDIA RTX4070
system. Compatible OpenGL remains preferred for systems where that path works.

Explicit backend choices, adapter-name filters and power preferences are
respected. If no suitable adapter is available, the startup error reports
available adapters and surface compatibility with driver/override guidance.

Tracking, camera settings, calibration, recording and the Mac/Windows graphics
defaults are unchanged. The walking-speed tracking investigation is a separate
follow-up.

The patch also includes the corrected UTF-8 packaging environment, complete
dependency/font notices and Debian GNU long-path headers already used for the
verified v0.3.1 installer. [Installation](https://pvnkmrksk.github.io/hejjetrak/manual/install.html)
and [graphics troubleshooting](https://pvnkmrksk.github.io/hejjetrak/manual/troubleshooting.html)
cover first launch and explicit overrides.

Created by [Pavan Kumar Kaushik](https://github.com/pvnkmrksk).
Based on [FicTrac](https://github.com/rjdmoore/fictrac).
