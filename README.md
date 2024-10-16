[![Build Status](https://badge.buildkite.com/9e0e6c88972a3248a0908506d6946624da84e4e18c0870c4d0.svg)](https://buildkite.com/rust-vmm/kvm-ioctls-ci)
[![crates.io](https://img.shields.io/crates/v/kvm-ioctls.svg)](https://crates.io/crates/kvm-ioctls)

# kvm-ioctls

The kvm-ioctls crate provides safe wrappers over the
[KVM API](https://www.kernel.org/doc/Documentation/virtual/kvm/api.txt), a set
of ioctls used for creating and configuring Virtual Machines (VMs) on Linux.
The ioctls are accessible through four structures:
- `Kvm` - wrappers over system ioctls
- `VmFd` - wrappers over VM ioctls
- `VcpuFd` - wrappers over vCPU ioctls
- `DeviceFd` - wrappers over device ioctls

For further details check the
[KVM API](https://www.kernel.org/doc/Documentation/virtual/kvm/api.txt) as well
as the code documentation.

## Supported Platforms

The kvm-ioctls can be used on x86_64, aarch64 and riscv64 (experimental).

## Running the tests

Our Continuous Integration (CI) pipeline is implemented on top of
[Buildkite](https://buildkite.com/).
For the complete list of tests, check our
[CI pipeline](https://buildkite.com/rust-vmm/kvm-ioctls-ci).

Each individual test runs in a container. To reproduce a test locally, you can
use the dev-container on x86_64, arm64 and riscv64.

```bash
# For running riscv64 tests, replace v47 with v47-riscv. This provides an
# emulated riscv64 environment on a x86_64 host.
docker run --device=/dev/kvm \
           -it \
           --security-opt seccomp=unconfined \
           --volume $(pwd)/kvm-ioctls:/kvm-ioctls \
           rustvmm/dev:v47
cd kvm-ioctls/
cargo test
```

For more details about the integration tests that are run for `kvm-ioctls`,
check the [rust-vmm-ci](https://github.com/rust-vmm/rust-vmm-ci) readme.
