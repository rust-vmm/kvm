// Copyright 2019 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::fs::File;
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};

use kvm_bindings::kvm_device_attr;

use kvm_ioctls::KVM_SET_DEVICE_ATTR;
use vmm_sys_util::errno;
use vmm_sys_util::ioctl::ioctl_with_ref;

/// A specialized `Result` type for device KVM ioctls.
///
/// This typedef is generally used to avoid writing out errno::Error directly and
/// is otherwise a direct mapping to Result.
///
/// This is temporary until all io::Errors have been converted to errno::Errors and will
/// be removed in a later commit. I've chosen to temporarily add it so that each individual
/// commit is buildable and functioning.
pub type Result<T> = std::result::Result<T, errno::Error>;

/// Wrapper over the file descriptor obtained when creating an emulated device in the kernel.
pub struct DeviceFd {
    fd: File,
}

impl DeviceFd {
    /// Sets a specified piece of device configuration and/or state.
    ///
    /// See the documentation for `KVM_SET_DEVICE_ATTR`.
    /// # Arguments
    ///
    /// * `device_attr` - The device attribute to be set.
    ///
    pub fn set_device_attr(&self, device_attr: &kvm_device_attr) -> Result<()> {
        let ret = unsafe { ioctl_with_ref(self, KVM_SET_DEVICE_ATTR(), device_attr) };
        if ret != 0 {
            return Err(errno::Error::last());
        }
        Ok(())
    }
}

/// Helper function for creating a new device.
pub fn new_device(dev_fd: File) -> DeviceFd {
    DeviceFd { fd: dev_fd }
}

impl AsRawFd for DeviceFd {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

impl FromRawFd for DeviceFd {
    /// This function is also unsafe as the primitives currently returned have the contract that
    /// they are the sole owner of the file descriptor they are wrapping. Usage of this function
    /// could accidentally allow violating this contract which can cause memory unsafety in code
    /// that relies on it being true.
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        DeviceFd {
            fd: File::from_raw_fd(fd),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ioctls::system::Kvm;
    use kvm_bindings::{
        kvm_device_type_KVM_DEV_TYPE_ARM_VGIC_V3, kvm_device_type_KVM_DEV_TYPE_VFIO,
        KVM_CREATE_DEVICE_TEST, KVM_DEV_VFIO_GROUP, KVM_DEV_VFIO_GROUP_ADD,
    };

    #[test]
    fn test_create_device() {
        #[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
        use kvm_bindings::kvm_device_type_KVM_DEV_TYPE_FSL_MPIC_20;

        let kvm = Kvm::new().unwrap();
        let vm = kvm.create_vm().unwrap();

        let mut gic_device = kvm_bindings::kvm_create_device {
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            type_: kvm_device_type_KVM_DEV_TYPE_ARM_VGIC_V3,
            #[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
            type_: kvm_device_type_KVM_DEV_TYPE_FSL_MPIC_20,
            fd: 0,
            flags: KVM_CREATE_DEVICE_TEST,
        };
        // This fails on x86_64 because there is no VGIC there.
        // This fails on aarch64 as it does not use MPIC (MultiProcessor Interrupt Controller), it uses
        // the VGIC.
        assert!(vm.create_device(&mut gic_device).is_err());

        if cfg!(any(target_arch = "x86", target_arch = "x86_64")) {
            gic_device.type_ = kvm_device_type_KVM_DEV_TYPE_VFIO;
        } else if cfg!(any(target_arch = "arm", target_arch = "aarch64")) {
            gic_device.type_ = kvm_device_type_KVM_DEV_TYPE_ARM_VGIC_V3;
        }
        let device_fd = vm
            .create_device(&mut gic_device)
            .expect("Cannot create KVM device");

        let raw_fd = unsafe { libc::dup(device_fd.as_raw_fd()) };
        assert!(raw_fd >= 0);
        let device_fd = unsafe { DeviceFd::from_raw_fd(raw_fd) };

        let dist_attr = kvm_bindings::kvm_device_attr {
            group: KVM_DEV_VFIO_GROUP,
            attr: KVM_DEV_VFIO_GROUP_ADD as u64,
            addr: 0x0,
            flags: 0,
        };

        // We are just creating a test device. Creating a real device would make the CI dependent
        // on host configuration (like having /dev/vfio). We expect this to fail.
        assert!(device_fd.set_device_attr(&dist_attr).is_err());
        assert_eq!(errno::Error::last().errno(), 25);
    }
}
