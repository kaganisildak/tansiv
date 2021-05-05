"""
Some common default.

Intent: gives some consistent settings between different scripts
"""
from pathlib import Path
import os


ROOT_DIR = Path(__file__).parent.parent.resolve()

# fallback on the system one
QEMU = os.environ.get("QEMU", "qemu-system-x86_64")

# fallback on the local built disk
QEMU_IMAGE = os.environ.get(
    "IMAGE",
    ROOT_DIR
    / "packer"
    / "packer-debian-10.3.0-x86_64-qemu"
    / "debian-10.3.0-x86_64.qcow2",
)

# fallback on our setting
QEMU_ARGS = os.environ.get(
    "QEMU_ARGS", "-icount shift=0,sleep=off,align=off -rtc clock=vm"
)

BOOT_CMD = os.environ.get("BOOT_CMD", ROOT_DIR / "examples" / "qemus" / "boot.py")

AUTOCONFIG_NET = False

DEFAULT_DOCKER_IMAGE = "registry.gitlab.inria.fr/quinson/2018-vsg/tansiv:latest"
