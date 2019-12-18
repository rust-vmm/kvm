// Copyright 2018 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT
//
// Portions Copyright 2017 The Chromium OS Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the THIRD-PARTY file.

use kvm_bindings::*;
use std::fs::File;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use std::os::raw::c_void;
use std::os::raw::{c_int, c_ulong};
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};

use ioctls::device::new_device;
use ioctls::device::DeviceFd;
use ioctls::vcpu::new_vcpu;
use ioctls::vcpu::VcpuFd;
use ioctls::{KvmRunWrapper, Result};
use kvm_ioctls::*;
use vmm_sys_util::errno;
use vmm_sys_util::eventfd::EventFd;
use vmm_sys_util::ioctl::{ioctl, ioctl_with_mut_ref, ioctl_with_ref, ioctl_with_val};

/// An address either in programmable I/O space or in memory mapped I/O space.
///
/// The `IoEventAddress` is used for specifying the type when registering an event
/// in [register_ioevent](struct.VmFd.html#method.register_ioevent).
///
pub enum IoEventAddress {
    /// Representation of an programmable I/O address.
    Pio(u64),
    /// Representation of an memory mapped I/O address.
    Mmio(u64),
}

/// Helper structure for disabling datamatch.
///
/// The structure can be used as a parameter to
/// [`register_ioevent`](struct.VmFd.html#method.register_ioevent)
/// to disable filtering of events based on the datamatch flag. For details check the
/// [KVM API documentation](https://www.kernel.org/doc/Documentation/virtual/kvm/api.txt).
///
pub struct NoDatamatch;
impl Into<u64> for NoDatamatch {
    fn into(self) -> u64 {
        0
    }
}

/// Wrapper over KVM VM ioctls.
pub struct VmFd {
    vm: File,
    run_size: usize,
}

impl VmFd {
    /// Creates/modifies a guest physical memory slot.
    ///
    /// See the documentation for `KVM_SET_USER_MEMORY_REGION`.
    ///
    /// # Arguments
    ///
    /// * `user_memory_region` - Guest physical memory slot. For details check the
    ///             `kvm_userspace_memory_region` structure in the
    ///             [KVM API doc](https://www.kernel.org/doc/Documentation/virtual/kvm/api.txt).
    ///
    /// # Safety
    ///
    /// This function is unsafe because there is no guarantee `userspace_addr` points to a valid
    /// memory region, nor the memory region lives as long as the kernel needs it to.
    ///
    /// The caller of this method must make sure that:
    /// - the raw pointer (`userspace_addr`) points to valid memory
    /// - the regions provided to KVM are not overlapping other memory regions.
    ///
    /// # Example
    ///
    /// ```rust
    /// # extern crate kvm_ioctls;
    /// extern crate kvm_bindings;
    ///
    /// use kvm_ioctls::Kvm;
    /// use kvm_bindings::kvm_userspace_memory_region;
    ///
    /// let kvm = Kvm::new().unwrap();
    /// let vm = kvm.create_vm().unwrap();
    /// let mem_region = kvm_userspace_memory_region {
    ///                     slot: 0,
    ///                     guest_phys_addr: 0x1000 as u64,
    ///                     memory_size: 0x4000 as u64,
    ///                     userspace_addr: 0x0 as u64,
    ///                     flags: 0,
    ///                 };
    /// unsafe {
    ///     vm.set_user_memory_region(mem_region).unwrap();
    /// };
    /// ```
    ///
    pub unsafe fn set_user_memory_region(
        &self,
        user_memory_region: kvm_userspace_memory_region,
    ) -> Result<()> {
        let ret = ioctl_with_ref(self, KVM_SET_USER_MEMORY_REGION(), &user_memory_region);
        if ret == 0 {
            Ok(())
        } else {
            Err(errno::Error::last())
        }
    }

    /// Sets the address of the three-page region in the VM's address space.
    ///
    /// See the documentation for `KVM_SET_TSS_ADDR`.
    ///
    /// # Arguments
    ///
    /// * `offset` - Physical address of a three-page region in the guest's physical address space.
    ///
    /// # Example
    ///
    /// ```rust
    /// # extern crate kvm_ioctls;
    /// # use kvm_ioctls::Kvm;
    /// let kvm = Kvm::new().unwrap();
    /// let vm = kvm.create_vm().unwrap();
    /// vm.set_tss_address(0xfffb_d000).unwrap();
    /// ```
    ///
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    pub fn set_tss_address(&self, offset: usize) -> Result<()> {
        // Safe because we know that our file is a VM fd and we verify the return result.
        let ret = unsafe { ioctl_with_val(self, KVM_SET_TSS_ADDR(), offset as c_ulong) };
        if ret == 0 {
            Ok(())
        } else {
            Err(errno::Error::last())
        }
    }

    /// Creates an in-kernel interrupt controller.
    ///
    /// See the documentation for `KVM_CREATE_IRQCHIP`.
    ///
    /// # Example
    ///
    /// ```rust
    /// # extern crate kvm_ioctls;
    /// # use kvm_ioctls::Kvm;
    /// let kvm = Kvm::new().unwrap();
    /// let vm = kvm.create_vm().unwrap();
    ///
    /// #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    /// vm.create_irq_chip().unwrap();
    /// ```
    ///
    #[cfg(any(
        target_arch = "x86",
        target_arch = "x86_64",
        target_arch = "arm",
        target_arch = "aarch64"
    ))]
    pub fn create_irq_chip(&self) -> Result<()> {
        // Safe because we know that our file is a VM fd and we verify the return result.
        let ret = unsafe { ioctl(self, KVM_CREATE_IRQCHIP()) };
        if ret == 0 {
            Ok(())
        } else {
            Err(errno::Error::last())
        }
    }

    /// X86 specific call to retrieve the state of a kernel interrupt controller.
    ///
    /// See the documentation for `KVM_GET_IRQCHIP` in the
    /// [KVM API doc](https://www.kernel.org/doc/Documentation/virtual/kvm/api.txt).
    ///
    /// # Arguments
    ///
    /// * `irqchip` - `kvm_irqchip` (input/output) to be read.
    ///
    /// # Example
    ///
    /// ```rust
    /// # extern crate kvm_bindings;
    /// # extern crate kvm_ioctls;
    /// # use kvm_bindings::{kvm_irqchip, KVM_IRQCHIP_PIC_MASTER};
    /// # use kvm_ioctls::Kvm;
    /// let kvm = Kvm::new().unwrap();
    /// let vm = kvm.create_vm().unwrap();
    ///
    /// vm.create_irq_chip().unwrap();
    /// let mut irqchip = kvm_irqchip::default();
    /// irqchip.chip_id = KVM_IRQCHIP_PIC_MASTER;
    /// vm.get_irqchip(&mut irqchip).unwrap();
    /// ```
    ///
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    pub fn get_irqchip(&self, irqchip: &mut kvm_irqchip) -> Result<()> {
        let ret = unsafe {
            // Here we trust the kernel not to read past the end of the kvm_irqchip struct.
            ioctl_with_mut_ref(self, KVM_GET_IRQCHIP(), irqchip)
        };
        if ret == 0 {
            Ok(())
        } else {
            Err(errno::Error::last())
        }
    }

    /// X86 specific call to set the state of a kernel interrupt controller.
    ///
    /// See the documentation for `KVM_SET_IRQCHIP` in the
    /// [KVM API doc](https://www.kernel.org/doc/Documentation/virtual/kvm/api.txt).
    ///
    /// # Arguments
    ///
    /// * `irqchip` - `kvm_irqchip` (input/output) to be written.
    ///
    /// # Example
    ///
    /// ```rust
    /// # extern crate kvm_bindings;
    /// # extern crate kvm_ioctls;
    /// # use kvm_bindings::{kvm_irqchip, KVM_IRQCHIP_PIC_MASTER};
    /// # use kvm_ioctls::Kvm;
    /// let kvm = Kvm::new().unwrap();
    /// let vm = kvm.create_vm().unwrap();
    ///
    /// vm.create_irq_chip().unwrap();
    /// let mut irqchip = kvm_irqchip::default();
    /// irqchip.chip_id = KVM_IRQCHIP_PIC_MASTER;
    /// // Your `irqchip` manipulation here.
    /// vm.set_irqchip(&mut irqchip).unwrap();
    /// ```
    ///
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    pub fn set_irqchip(&self, irqchip: &kvm_irqchip) -> Result<()> {
        let ret = unsafe {
            // Here we trust the kernel not to read past the end of the kvm_irqchip struct.
            ioctl_with_ref(self, KVM_SET_IRQCHIP(), irqchip)
        };
        if ret == 0 {
            Ok(())
        } else {
            Err(errno::Error::last())
        }
    }

    /// Creates a PIT as per the `KVM_CREATE_PIT2` ioctl.
    ///
    /// # Arguments
    ///
    /// * pit_config - PIT configuration. For details check the `kvm_pit_config` structure in the
    ///                [KVM API doc](https://www.kernel.org/doc/Documentation/virtual/kvm/api.txt).
    /// # Example
    ///
    /// ```rust
    /// # extern crate kvm_ioctls;
    /// extern crate kvm_bindings;
    /// # use kvm_ioctls::Kvm;
    /// use kvm_bindings::kvm_pit_config;
    ///
    /// let kvm = Kvm::new().unwrap();
    /// let vm = kvm.create_vm().unwrap();
    /// let pit_config = kvm_pit_config::default();
    /// vm.create_pit2(pit_config).unwrap();
    /// ```
    ///
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    pub fn create_pit2(&self, pit_config: kvm_pit_config) -> Result<()> {
        // Safe because we know that our file is a VM fd, we know the kernel will only read the
        // correct amount of memory from our pointer, and we verify the return result.
        let ret = unsafe { ioctl_with_ref(self, KVM_CREATE_PIT2(), &pit_config) };
        if ret == 0 {
            Ok(())
        } else {
            Err(errno::Error::last())
        }
    }

    /// X86 specific call to retrieve the state of the in-kernel PIT model.
    ///
    /// See the documentation for `KVM_GET_PIT2` in the
    /// [KVM API doc](https://www.kernel.org/doc/Documentation/virtual/kvm/api.txt).
    ///
    /// # Arguments
    ///
    /// * `pitstate` - `kvm_pit_state2` to be read.
    ///
    /// # Example
    ///
    /// ```rust
    /// # extern crate kvm_bindings;
    /// # extern crate kvm_ioctls;
    /// # use kvm_bindings::kvm_pit_config;
    /// # use kvm_ioctls::Kvm;
    /// let kvm = Kvm::new().unwrap();
    /// let vm = kvm.create_vm().unwrap();
    ///
    /// let pit_config = kvm_pit_config::default();
    /// vm.create_pit2(pit_config).unwrap();
    /// let pitstate = vm.get_pit2().unwrap();
    /// ```
    ///
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    pub fn get_pit2(&self) -> Result<kvm_pit_state2> {
        let mut pitstate = Default::default();
        let ret = unsafe {
            // Here we trust the kernel not to read past the end of the kvm_pit_state2 struct.
            ioctl_with_mut_ref(self, KVM_GET_PIT2(), &mut pitstate)
        };
        if ret == 0 {
            Ok(pitstate)
        } else {
            Err(errno::Error::last())
        }
    }

    /// X86 specific call to set the state of the in-kernel PIT model.
    ///
    /// See the documentation for `KVM_SET_PIT2` in the
    /// [KVM API doc](https://www.kernel.org/doc/Documentation/virtual/kvm/api.txt).
    ///
    /// # Arguments
    ///
    /// * `pitstate` - `kvm_pit_state2` to be written.
    ///
    /// # Example
    ///
    /// ```rust
    /// # extern crate kvm_bindings;
    /// # extern crate kvm_ioctls;
    /// # use kvm_bindings::{kvm_pit_config, kvm_pit_state2};
    /// # use kvm_ioctls::Kvm;
    /// let kvm = Kvm::new().unwrap();
    /// let vm = kvm.create_vm().unwrap();
    ///
    /// let pit_config = kvm_pit_config::default();
    /// vm.create_pit2(pit_config).unwrap();
    /// let mut pitstate = kvm_pit_state2::default();
    /// // Your `pitstate` manipulation here.
    /// vm.set_pit2(&mut pitstate).unwrap();
    /// ```
    ///
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    pub fn set_pit2(&self, pitstate: &kvm_pit_state2) -> Result<()> {
        let ret = unsafe {
            // Here we trust the kernel not to read past the end of the kvm_pit_state2 struct.
            ioctl_with_ref(self, KVM_SET_PIT2(), pitstate)
        };
        if ret == 0 {
            Ok(())
        } else {
            Err(errno::Error::last())
        }
    }

    /// X86 specific call to retrieve the current timestamp of kvmclock.
    ///
    /// See the documentation for `KVM_GET_CLOCK` in the
    /// [KVM API doc](https://www.kernel.org/doc/Documentation/virtual/kvm/api.txt).
    ///
    /// # Arguments
    ///
    /// * `clock` - `kvm_clock_data` to be read.
    ///
    /// # Example
    ///
    /// ```rust
    /// # extern crate kvm_ioctls;
    /// # use kvm_ioctls::Kvm;
    /// let kvm = Kvm::new().unwrap();
    /// let vm = kvm.create_vm().unwrap();
    /// let clock = vm.get_clock().unwrap();
    /// ```
    ///
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    pub fn get_clock(&self) -> Result<kvm_clock_data> {
        let mut clock = Default::default();
        let ret = unsafe {
            // Here we trust the kernel not to read past the end of the kvm_clock_data struct.
            ioctl_with_mut_ref(self, KVM_GET_CLOCK(), &mut clock)
        };
        if ret == 0 {
            Ok(clock)
        } else {
            Err(errno::Error::last())
        }
    }

    /// X86 specific call to set the current timestamp of kvmclock.
    ///
    /// See the documentation for `KVM_SET_CLOCK` in the
    /// [KVM API doc](https://www.kernel.org/doc/Documentation/virtual/kvm/api.txt).
    ///
    /// # Arguments
    ///
    /// * `clock` - `kvm_clock_data` to be written.
    ///
    /// # Example
    ///
    /// ```rust
    /// # extern crate kvm_bindings;
    /// # extern crate kvm_ioctls;
    /// # use kvm_bindings::kvm_clock_data;
    /// # use kvm_ioctls::Kvm;
    /// let kvm = Kvm::new().unwrap();
    /// let vm = kvm.create_vm().unwrap();
    /// let mut clock = kvm_clock_data::default();
    /// vm.set_clock(&mut clock).unwrap();
    /// ```
    ///
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    pub fn set_clock(&self, clock: &kvm_clock_data) -> Result<()> {
        let ret = unsafe {
            // Here we trust the kernel not to read past the end of the kvm_clock_data struct.
            ioctl_with_ref(self, KVM_SET_CLOCK(), clock)
        };
        if ret == 0 {
            Ok(())
        } else {
            Err(errno::Error::last())
        }
    }

    /// Directly injects a MSI message as per the `KVM_SIGNAL_MSI` ioctl.
    ///
    /// See the documentation for `KVM_SIGNAL_MSI`.
    ///
    /// This ioctl returns > 0 when the MSI is successfully delivered and 0
    /// when the guest blocked the MSI.
    ///
    /// # Arguments
    ///
    /// * kvm_msi - MSI message configuration. For details check the `kvm_msi` structure in the
    ///                [KVM API doc](https://www.kernel.org/doc/Documentation/virtual/kvm/api.txt).
    /// # Example
    ///
    /// In this example, the important function signal_msi() calling into
    /// the actual ioctl is commented out. The reason is that MSI vectors are
    /// not chosen from the HW side (VMM). The guest OS (or anything that runs
    /// inside the VM) is supposed to allocate the MSI vectors, and usually
    /// communicate back through PCI configuration space. Sending a random MSI
    /// vector through this signal_msi() function will always result in a
    /// failure, which is why it needs to be commented out.
    ///
    /// ```rust
    /// # extern crate kvm_ioctls;
    /// extern crate kvm_bindings;
    /// # use kvm_ioctls::Kvm;
    /// use kvm_bindings::kvm_msi;
    ///
    /// let kvm = Kvm::new().unwrap();
    /// let vm = kvm.create_vm().unwrap();
    /// let msi = kvm_msi::default();
    /// #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    /// vm.create_irq_chip().unwrap();
    /// //vm.signal_msi(msi).unwrap();
    /// ```
    ///
    #[cfg(any(
        target_arch = "x86",
        target_arch = "x86_64",
        target_arch = "arm",
        target_arch = "aarch64"
    ))]
    pub fn signal_msi(&self, msi: kvm_msi) -> Result<c_int> {
        // Safe because we allocated the structure and we know the kernel
        // will read exactly the size of the structure.
        let ret = unsafe { ioctl_with_ref(self, KVM_SIGNAL_MSI(), &msi) };
        if ret >= 0 {
            Ok(ret)
        } else {
            Err(errno::Error::last())
        }
    }

    /// Sets the GSI routing table entries, overwriting any previously set
    /// entries, as per the `KVM_SET_GSI_ROUTING` ioctl.
    ///
    /// See the documentation for `KVM_SET_GSI_ROUTING`.
    ///
    /// Returns an io::Error when the table could not be updated.
    ///
    /// # Arguments
    ///
    /// * kvm_irq_routing - IRQ routing configuration. Describe all routes
    ///                     associated with GSI entries. For details check
    ///                     the `kvm_irq_routing` and `kvm_irq_routing_entry`
    ///                     structures in the
    ///                     [KVM API doc](https://www.kernel.org/doc/Documentation/virtual/kvm/api.txt).
    /// # Example
    ///
    /// ```rust
    /// # extern crate kvm_ioctls;
    /// extern crate kvm_bindings;
    /// # use kvm_ioctls::Kvm;
    /// use kvm_bindings::kvm_irq_routing;
    ///
    /// let kvm = Kvm::new().unwrap();
    /// let vm = kvm.create_vm().unwrap();
    /// #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    /// vm.create_irq_chip().unwrap();
    ///
    /// let irq_routing = kvm_irq_routing::default();
    /// vm.set_gsi_routing(&irq_routing).unwrap();
    /// ```
    ///
    #[cfg(any(
        target_arch = "x86",
        target_arch = "x86_64",
        target_arch = "arm",
        target_arch = "aarch64"
    ))]
    pub fn set_gsi_routing(&self, irq_routing: &kvm_irq_routing) -> Result<()> {
        // Safe because we allocated the structure and we know the kernel
        // will read exactly the size of the structure.
        let ret = unsafe { ioctl_with_ref(self, KVM_SET_GSI_ROUTING(), irq_routing) };
        if ret == 0 {
            Ok(())
        } else {
            Err(errno::Error::last())
        }
    }

    /// Registers an event to be signaled whenever a certain address is written to.
    ///
    /// See the documentation for `KVM_IOEVENTFD`.
    ///
    /// # Arguments
    ///
    /// * `fd` - `EventFd` which will be signaled. When signaling, the usual `vmexit` to userspace
    ///           is prevented.
    /// * `addr` - Address being written to.
    /// * `datamatch` - Limits signaling `fd` to only the cases where the value being written is
    ///                 equal to this parameter. The size of `datamatch` is important and it must
    ///                 match the expected size of the guest's write.
    ///
    /// # Example
    ///
    /// ```rust
    /// # extern crate kvm_ioctls;
    /// extern crate libc;
    /// extern crate vmm_sys_util;
    /// # use kvm_ioctls::{IoEventAddress, Kvm, NoDatamatch};
    /// use libc::{eventfd, EFD_NONBLOCK};
    /// use vmm_sys_util::eventfd::EventFd;
    /// let kvm = Kvm::new().unwrap();
    /// let vm_fd = kvm.create_vm().unwrap();
    /// let evtfd = EventFd::new(EFD_NONBLOCK).unwrap();
    /// vm_fd
    ///    .register_ioevent(&evtfd, &IoEventAddress::Pio(0xf4), NoDatamatch)
    ///    .unwrap();
    /// vm_fd
    ///    .register_ioevent(&evtfd, &IoEventAddress::Mmio(0x1000), NoDatamatch)
    ///    .unwrap();
    /// ```
    ///
    pub fn register_ioevent<T: Into<u64>>(
        &self,
        fd: &EventFd,
        addr: &IoEventAddress,
        datamatch: T,
    ) -> Result<()> {
        let mut flags = 0;
        if std::mem::size_of::<T>() > 0 {
            flags |= 1 << kvm_ioeventfd_flag_nr_datamatch
        }
        if let IoEventAddress::Pio(_) = *addr {
            flags |= 1 << kvm_ioeventfd_flag_nr_pio
        }

        let ioeventfd = kvm_ioeventfd {
            datamatch: datamatch.into(),
            len: std::mem::size_of::<T>() as u32,
            addr: match addr {
                IoEventAddress::Pio(ref p) => *p as u64,
                IoEventAddress::Mmio(ref m) => *m,
            },
            fd: fd.as_raw_fd(),
            flags,
            ..Default::default()
        };
        // Safe because we know that our file is a VM fd, we know the kernel will only read the
        // correct amount of memory from our pointer, and we verify the return result.
        let ret = unsafe { ioctl_with_ref(self, KVM_IOEVENTFD(), &ioeventfd) };
        if ret == 0 {
            Ok(())
        } else {
            Err(errno::Error::last())
        }
    }

    /// Unregisters an event from a certain address it has been previously registered to.
    ///
    /// See the documentation for `KVM_IOEVENTFD`.
    ///
    /// # Arguments
    ///
    /// * `fd` - FD which will be unregistered.
    /// * `addr` - Address being written to.
    ///
    /// # Safety
    ///
    /// This function is unsafe because it relies on RawFd.
    ///
    /// # Example
    ///
    /// ```rust
    /// # extern crate kvm_ioctls;
    /// extern crate libc;
    /// extern crate vmm_sys_util;
    /// # use kvm_ioctls::{IoEventAddress, Kvm, NoDatamatch};
    /// use libc::EFD_NONBLOCK;
    /// use vmm_sys_util::eventfd::EventFd;
    ///
    /// let kvm = Kvm::new().unwrap();
    /// let vm_fd = kvm.create_vm().unwrap();
    /// let evtfd = EventFd::new(EFD_NONBLOCK).unwrap();
    /// let pio_addr = IoEventAddress::Pio(0xf4);
    /// let mmio_addr = IoEventAddress::Mmio(0x1000);
    /// vm_fd
    ///    .register_ioevent(&evtfd, &pio_addr, NoDatamatch)
    ///    .unwrap();
    /// vm_fd
    ///    .register_ioevent(&evtfd, &mmio_addr, NoDatamatch)
    ///    .unwrap();
    /// vm_fd
    ///    .unregister_ioevent(&evtfd, &pio_addr)
    ///    .unwrap();
    /// vm_fd
    ///    .unregister_ioevent(&evtfd, &mmio_addr)
    ///    .unwrap();
    /// ```
    ///
    pub fn unregister_ioevent(&self, fd: &EventFd, addr: &IoEventAddress) -> Result<()> {
        let mut flags = 1 << kvm_ioeventfd_flag_nr_deassign;
        if let IoEventAddress::Pio(_) = *addr {
            flags |= 1 << kvm_ioeventfd_flag_nr_pio
        }

        let ioeventfd = kvm_ioeventfd {
            addr: match addr {
                IoEventAddress::Pio(ref p) => *p as u64,
                IoEventAddress::Mmio(ref m) => *m,
            },
            fd: fd.as_raw_fd(),
            flags,
            ..Default::default()
        };
        // Safe because we know that our file is a VM fd, we know the kernel will only read the
        // correct amount of memory from our pointer, and we verify the return result.
        let ret = unsafe { ioctl_with_ref(self, KVM_IOEVENTFD(), &ioeventfd) };
        if ret == 0 {
            Ok(())
        } else {
            Err(errno::Error::last())
        }
    }

    /// Gets the bitmap of pages dirtied since the last call of this function.
    ///
    /// Leverages the dirty page logging feature in KVM. As a side-effect, this also resets the
    /// bitmap inside the kernel. For the dirty log to be available, you have to set the flag
    /// `KVM_MEM_LOG_DIRTY_PAGES` when creating guest memory regions.
    ///
    /// Check the documentation for `KVM_GET_DIRTY_LOG`.
    ///
    /// # Arguments
    ///
    /// * `slot` - Guest memory slot identifier.
    /// * `memory_size` - Size of the memory region.
    ///
    /// # Example
    ///
    /// ```rust
    /// # extern crate kvm_ioctls;
    /// # extern crate kvm_bindings;
    /// # use std::io::Write;
    /// # use std::ptr::null_mut;
    /// # use std::slice;
    /// # use kvm_ioctls::{Kvm, VcpuExit};
    /// # use kvm_bindings::{kvm_userspace_memory_region, KVM_MEM_LOG_DIRTY_PAGES};
    /// # let kvm = Kvm::new().unwrap();
    /// # let vm = kvm.create_vm().unwrap();
    /// // This examples is based on https://lwn.net/Articles/658511/.
    /// let mem_size = 0x4000;
    /// let guest_addr: u64 = 0x1000;
    /// let load_addr: *mut u8 = unsafe {
    ///     libc::mmap(
    ///         null_mut(),
    ///         mem_size,
    ///         libc::PROT_READ | libc::PROT_WRITE,
    ///         libc::MAP_ANONYMOUS | libc::MAP_SHARED | libc::MAP_NORESERVE,
    ///         -1,
    ///         0,
    ///     ) as *mut u8
    /// };
    ///
    /// // Initialize a guest memory region using the flag `KVM_MEM_LOG_DIRTY_PAGES`.
    /// let mem_region = kvm_userspace_memory_region {
    ///     slot: 0,
    ///     guest_phys_addr: guest_addr,
    ///     memory_size: mem_size as u64,
    ///     userspace_addr: load_addr as u64,
    ///     flags: KVM_MEM_LOG_DIRTY_PAGES,
    /// };
    /// unsafe { vm.set_user_memory_region(mem_region).unwrap() };
    ///
    /// // Dummy x86 code that just calls halt.
    /// let x86_code = [
    ///         0xf4,             /* hlt */
    /// ];
    ///
    /// // Write the code in the guest memory. This will generate a dirty page.
    /// unsafe {
    ///     let mut slice = slice::from_raw_parts_mut(load_addr, mem_size);
    ///     slice.write(&x86_code).unwrap();
    /// }
    ///
    /// let vcpu_fd = vm.create_vcpu(0).unwrap();
    ///
    /// let mut vcpu_sregs = vcpu_fd.get_sregs().unwrap();
    /// vcpu_sregs.cs.base = 0;
    /// vcpu_sregs.cs.selector = 0;
    /// vcpu_fd.set_sregs(&vcpu_sregs).unwrap();
    ///
    /// let mut vcpu_regs = vcpu_fd.get_regs().unwrap();
    ///  // Set the Instruction Pointer to the guest address where we loaded the code.
    /// vcpu_regs.rip = guest_addr;
    /// vcpu_regs.rax = 2;
    /// vcpu_regs.rbx = 3;
    /// vcpu_regs.rflags = 2;
    /// vcpu_fd.set_regs(&vcpu_regs).unwrap();
    ///
    /// loop {
    ///     match vcpu_fd.run().expect("run failed") {
    ///         VcpuExit::Hlt => {
    ///             // The code snippet dirties 1 page when loading the code in memory.
    ///             let dirty_pages_bitmap = vm.get_dirty_log(0, mem_size).unwrap();
    ///             let dirty_pages = dirty_pages_bitmap
    ///                     .into_iter()
    ///                     .map(|page| page.count_ones())
    ///                     .fold(0, |dirty_page_count, i| dirty_page_count + i);
    ///             assert_eq!(dirty_pages, 1);
    ///             break;
    ///         }
    ///         exit_reason => panic!("unexpected exit reason: {:?}", exit_reason),
    ///     }
    /// }
    /// ```
    ///
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    pub fn get_dirty_log(&self, slot: u32, memory_size: usize) -> Result<Vec<u64>> {
        // Compute the length of the bitmap needed for all dirty pages in one memory slot.
        // One memory page is 4KiB (4096 bits) and `KVM_GET_DIRTY_LOG` returns one dirty bit for
        // each page.
        let page_size = 4 << 10;

        let div_round_up = |dividend, divisor| (dividend + divisor - 1) / divisor;
        // For ease of access we are saving the bitmap in a u64 vector. We are using ceil to
        // make sure we count all dirty pages even when `mem_size` is not a multiple of
        // page_size * 64.
        let bitmap_size = div_round_up(memory_size, page_size * 64);
        let mut bitmap = vec![0; bitmap_size];
        let b_data = bitmap.as_mut_ptr() as *mut c_void;
        let dirtylog = kvm_dirty_log {
            slot,
            padding1: 0,
            __bindgen_anon_1: kvm_dirty_log__bindgen_ty_1 {
                dirty_bitmap: b_data,
            },
        };
        // Safe because we know that our file is a VM fd, and we know that the amount of memory
        // we allocated for the bitmap is at least one bit per page.
        let ret = unsafe { ioctl_with_ref(self, KVM_GET_DIRTY_LOG(), &dirtylog) };
        if ret == 0 {
            Ok(bitmap)
        } else {
            Err(errno::Error::last())
        }
    }

    /// Registers an event that will, when signaled, trigger the `gsi` IRQ.
    ///
    /// # Arguments
    ///
    /// * `fd` - `EventFd` to be signaled.
    /// * `gsi` - IRQ to be triggered.
    ///
    /// # Example
    ///
    /// ```rust
    /// # extern crate kvm_ioctls;
    /// # extern crate libc;
    /// # extern crate vmm_sys_util;
    /// # use kvm_ioctls::Kvm;
    /// # use libc::EFD_NONBLOCK;
    /// # use vmm_sys_util::eventfd::EventFd;
    /// let kvm = Kvm::new().unwrap();
    /// let vm = kvm.create_vm().unwrap();
    /// let evtfd = EventFd::new(EFD_NONBLOCK).unwrap();
    /// #[cfg(any(target_arch = "x86", target_arch = "x86_64"))] {
    ///     vm.create_irq_chip().unwrap();
    ///     vm.register_irqfd(&evtfd, 0).unwrap();
    /// }
    /// ```
    ///
    #[cfg(any(
        target_arch = "x86",
        target_arch = "x86_64",
        target_arch = "arm",
        target_arch = "aarch64"
    ))]
    pub fn register_irqfd(&self, fd: &EventFd, gsi: u32) -> Result<()> {
        let irqfd = kvm_irqfd {
            fd: fd.as_raw_fd() as u32,
            gsi,
            ..Default::default()
        };
        // Safe because we know that our file is a VM fd, we know the kernel will only read the
        // correct amount of memory from our pointer, and we verify the return result.
        let ret = unsafe { ioctl_with_ref(self, KVM_IRQFD(), &irqfd) };
        if ret == 0 {
            Ok(())
        } else {
            Err(errno::Error::last())
        }
    }

    /// Unregisters an event that will, when signaled, trigger the `gsi` IRQ.
    ///
    /// # Arguments
    ///
    /// * `fd` - `EventFd` to be signaled.
    /// * `gsi` - IRQ to be triggered.
    ///
    /// # Example
    ///
    /// ```rust
    /// # extern crate kvm_ioctls;
    /// # extern crate libc;
    /// # extern crate vmm_sys_util;
    /// # use kvm_ioctls::Kvm;
    /// # use libc::EFD_NONBLOCK;
    /// # use vmm_sys_util::eventfd::EventFd;
    /// let kvm = Kvm::new().unwrap();
    /// let vm = kvm.create_vm().unwrap();
    /// let evtfd = EventFd::new(EFD_NONBLOCK).unwrap();
    /// #[cfg(any(target_arch = "x86", target_arch = "x86_64"))] {
    ///     vm.create_irq_chip().unwrap();
    ///     vm.register_irqfd(&evtfd, 0).unwrap();
    ///     vm.unregister_irqfd(&evtfd, 0).unwrap();
    /// }
    /// ```
    ///
    #[cfg(any(
        target_arch = "x86",
        target_arch = "x86_64",
        target_arch = "arm",
        target_arch = "aarch64"
    ))]
    pub fn unregister_irqfd(&self, fd: &EventFd, gsi: u32) -> Result<()> {
        let irqfd = kvm_irqfd {
            fd: fd.as_raw_fd() as u32,
            gsi,
            flags: KVM_IRQFD_FLAG_DEASSIGN,
            ..Default::default()
        };
        // Safe because we know that our file is a VM fd, we know the kernel will only read the
        // correct amount of memory from our pointer, and we verify the return result.
        let ret = unsafe { ioctl_with_ref(self, KVM_IRQFD(), &irqfd) };
        if ret == 0 {
            Ok(())
        } else {
            Err(errno::Error::last())
        }
    }

    /// Creates a new KVM vCPU file descriptor and maps the memory corresponding
    /// its `kvm_run` structure.
    ///
    /// See the documentation for `KVM_CREATE_VCPU`.
    ///
    /// # Arguments
    ///
    /// * `id` - The vCPU ID.
    ///
    /// # Errors
    ///
    /// Returns an io::Error when the VM fd is invalid or the vCPU memory cannot
    /// be mapped correctly.
    ///
    /// # Example
    ///
    /// ```rust
    /// # extern crate kvm_ioctls;
    /// # use kvm_ioctls::Kvm;
    /// let kvm = Kvm::new().unwrap();
    /// let vm = kvm.create_vm().unwrap();
    /// // Create one vCPU with the ID=0.
    /// let vcpu = vm.create_vcpu(0);
    /// ```
    ///
    pub fn create_vcpu(&self, id: u8) -> Result<VcpuFd> {
        // Safe because we know that vm is a VM fd and we verify the return result.
        #[allow(clippy::cast_lossless)]
        let vcpu_fd = unsafe { ioctl_with_val(&self.vm, KVM_CREATE_VCPU(), id as c_ulong) };
        if vcpu_fd < 0 {
            return Err(errno::Error::last());
        }

        // Wrap the vCPU now in case the following ? returns early. This is safe because we verified
        // the value of the fd and we own the fd.
        let vcpu = unsafe { File::from_raw_fd(vcpu_fd) };

        let kvm_run_ptr = KvmRunWrapper::mmap_from_fd(&vcpu, self.run_size)?;

        Ok(new_vcpu(vcpu, kvm_run_ptr))
    }

    /// Creates a VcpuFd object from a vcpu RawFd.
    ///
    /// This function is unsafe as the primitives currently returned have the contract that
    /// they are the sole owner of the file descriptor they are wrapping. Usage of this function
    /// could accidentally allow violating this contract which can cause memory unsafety in code
    /// that relies on it being true.
    ///
    /// # Example
    ///
    /// ```rust
    /// # extern crate kvm_ioctls;
    /// # use std::os::unix::io::AsRawFd;
    /// # use kvm_ioctls::{Kvm, VmFd, VcpuFd};
    /// let kvm = Kvm::new().unwrap();
    /// let vm = kvm.create_vm().unwrap();
    /// // Create one vCPU with the ID=0.
    /// let vcpu = vm.create_vcpu(0).unwrap();
    /// let rawfd = unsafe { libc::dup(vcpu.as_raw_fd()) };
    /// assert!(rawfd >= 0);
    /// let vcpu = unsafe { vm.create_vcpu_from_rawfd(rawfd).unwrap() };
    /// ```
    ///
    pub unsafe fn create_vcpu_from_rawfd(&self, fd: RawFd) -> Result<VcpuFd> {
        let vcpu = File::from_raw_fd(fd);
        let kvm_run_ptr = KvmRunWrapper::mmap_from_fd(&vcpu, self.run_size)?;
        Ok(new_vcpu(vcpu, kvm_run_ptr))
    }

    /// Creates an emulated device in the kernel.
    ///
    /// See the documentation for `KVM_CREATE_DEVICE`.
    ///
    /// # Arguments
    ///
    /// * `device`: device configuration. For details check the `kvm_create_device` structure in the
    ///                [KVM API doc](https://www.kernel.org/doc/Documentation/virtual/kvm/api.txt).
    ///
    /// # Example
    ///
    /// ```rust
    /// # extern crate kvm_ioctls;
    /// # extern crate kvm_bindings;
    /// # use kvm_ioctls::Kvm;
    /// use kvm_bindings::{
    ///     kvm_device_type_KVM_DEV_TYPE_VFIO,
    ///     kvm_device_type_KVM_DEV_TYPE_ARM_VGIC_V3,
    ///     KVM_CREATE_DEVICE_TEST,
    /// };
    /// let kvm = Kvm::new().unwrap();
    /// let vm = kvm.create_vm().unwrap();
    ///
    /// // Creating a device with the KVM_CREATE_DEVICE_TEST flag to check
    /// // whether the device type is supported. This will not create the device.
    /// // To create the device the flag needs to be removed.
    /// let mut device = kvm_bindings::kvm_create_device {
    ///     #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    ///     type_: kvm_device_type_KVM_DEV_TYPE_VFIO,
    ///     #[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
    ///     type_: kvm_device_type_KVM_DEV_TYPE_ARM_VGIC_V3,
    ///     fd: 0,
    ///     flags: KVM_CREATE_DEVICE_TEST,
    /// };
    /// let device_fd = vm
    ///     .create_device(&mut device).unwrap();
    /// ```
    ///
    pub fn create_device(&self, device: &mut kvm_create_device) -> Result<DeviceFd> {
        let ret = unsafe { ioctl_with_ref(self, KVM_CREATE_DEVICE(), device) };
        if ret == 0 {
            Ok(new_device(unsafe { File::from_raw_fd(device.fd as i32) }))
        } else {
            Err(errno::Error::last())
        }
    }

    /// Returns the preferred CPU target type which can be emulated by KVM on underlying host.
    ///
    /// The preferred CPU target is returned in the `kvi` parameter.
    /// See documentation for `KVM_ARM_PREFERRED_TARGET`.
    ///
    /// # Arguments
    /// * `kvi` - CPU target configuration (out). For details check the `kvm_vcpu_init`
    ///           structure in the
    ///           [KVM API doc](https://www.kernel.org/doc/Documentation/virtual/kvm/api.txt).
    ///
    /// # Example
    ///
    /// ```rust
    /// # extern crate kvm_ioctls;
    /// # extern crate kvm_bindings;
    /// # use kvm_ioctls::Kvm;
    /// use kvm_bindings::kvm_vcpu_init;
    /// let kvm = Kvm::new().unwrap();
    /// let vm = kvm.create_vm().unwrap();
    /// let mut kvi = kvm_vcpu_init::default();
    /// vm.get_preferred_target(&mut kvi).unwrap();
    /// ```
    ///
    #[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
    pub fn get_preferred_target(&self, kvi: &mut kvm_vcpu_init) -> Result<()> {
        // The ioctl is safe because we allocated the struct and we know the
        // kernel will write exactly the size of the struct.
        let ret = unsafe { ioctl_with_mut_ref(self, KVM_ARM_PREFERRED_TARGET(), kvi) };
        if ret != 0 {
            return Err(errno::Error::last());
        }
        Ok(())
    }

    /// Enable the specified capability as per the `KVM_ENABLE_CAP` ioctl.
    ///
    /// See the documentation for `KVM_ENABLE_CAP`.
    ///
    /// Returns an io::Error when the capability could not be enabled.
    ///
    /// # Arguments
    ///
    /// * kvm_enable_cap - KVM capability structure. For details check the `kvm_enable_cap`
    ///                    structure in the
    ///                    [KVM API doc](https://www.kernel.org/doc/Documentation/virtual/kvm/api.txt).
    ///
    /// # Example
    ///
    /// ```rust
    /// # extern crate kvm_ioctls;
    /// extern crate kvm_bindings;
    ///
    /// # use kvm_ioctls::Kvm;
    /// use kvm_bindings::{kvm_enable_cap, KVM_CAP_SPLIT_IRQCHIP};
    ///
    /// let kvm = Kvm::new().unwrap();
    /// let vm = kvm.create_vm().unwrap();
    /// let mut cap: kvm_enable_cap = Default::default();
    /// // This example cannot enable an arm/aarch64 capability since there
    /// // is no capability available for these architectures.
    /// if cfg!(target_arch = "x86") || cfg!(target_arch = "x86_64") {
    ///     cap.cap = KVM_CAP_SPLIT_IRQCHIP;
    ///     // As per the KVM documentation, KVM_CAP_SPLIT_IRQCHIP only emulates
    ///     // the local APIC in kernel, expecting that a userspace IOAPIC will
    ///     // be implemented by the VMM.
    ///     // Along with this capability, the user needs to specify the number
    ///     // of pins reserved for the userspace IOAPIC. This number needs to be
    ///     // provided through the first argument of the capability structure, as
    ///     // specified in KVM documentation:
    ///     //     args[0] - number of routes reserved for userspace IOAPICs
    ///     //
    ///     // Because an IOAPIC supports 24 pins, that's the reason why this test
    ///     // picked this number as reference.
    ///     cap.args[0] = 24;
    ///     vm.enable_cap(&cap).unwrap();
    /// }
    /// ```
    ///
    #[cfg(not(any(target_arch = "arm", target_arch = "aarch64")))]
    pub fn enable_cap(&self, cap: &kvm_enable_cap) -> Result<()> {
        // The ioctl is safe because we allocated the struct and we know the
        // kernel will write exactly the size of the struct.
        let ret = unsafe { ioctl_with_ref(self, KVM_ENABLE_CAP(), cap) };
        if ret == 0 {
            Ok(())
        } else {
            Err(errno::Error::last())
        }
    }

    /// Get the `kvm_run` size.
    pub fn run_size(&self) -> usize {
        self.run_size
    }
}

/// Helper function to create a new `VmFd`.
///
/// This should not be exported as a public function because the preferred way is to use
/// `create_vm` from `Kvm`. The function cannot be part of the `VmFd` implementation because
/// then it would be exported with the public `VmFd` interface.
pub fn new_vmfd(vm: File, run_size: usize) -> VmFd {
    VmFd { vm, run_size }
}

impl AsRawFd for VmFd {
    fn as_raw_fd(&self) -> RawFd {
        self.vm.as_raw_fd()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Kvm;

    use libc::EFD_NONBLOCK;

    #[test]
    fn test_set_invalid_memory() {
        let kvm = Kvm::new().unwrap();
        let vm = kvm.create_vm().unwrap();
        let invalid_mem_region = kvm_userspace_memory_region {
            slot: 0,
            guest_phys_addr: 0,
            memory_size: 0,
            userspace_addr: 0,
            flags: 0,
        };
        assert!(unsafe { vm.set_user_memory_region(invalid_mem_region) }.is_err());
    }

    #[test]
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    fn test_set_tss_address() {
        let kvm = Kvm::new().unwrap();
        let vm = kvm.create_vm().unwrap();
        assert!(vm.set_tss_address(0xfffb_d000).is_ok());
    }

    #[test]
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    fn test_irq_chip() {
        use Cap;

        let kvm = Kvm::new().unwrap();
        assert!(kvm.check_extension(Cap::Irqchip));
        let vm = kvm.create_vm().unwrap();
        assert!(vm.create_irq_chip().is_ok());

        let mut irqchip = kvm_irqchip::default();
        irqchip.chip_id = KVM_IRQCHIP_PIC_MASTER;
        // Set the irq_base to a non-default value to check that set & get work.
        unsafe {
            irqchip.chip.pic.irq_base = 10;
        }
        assert!(vm.set_irqchip(&irqchip).is_ok());

        // We initialize a dummy irq chip (`other_irqchip`) in which the
        // function `get_irqchip` returns its result.
        let mut other_irqchip = kvm_irqchip::default();
        other_irqchip.chip_id = KVM_IRQCHIP_PIC_MASTER;
        assert!(vm.get_irqchip(&mut other_irqchip).is_ok());

        // Safe because we know that the irqchip type is PIC.
        unsafe { assert_eq!(irqchip.chip.pic, other_irqchip.chip.pic) };
    }

    #[test]
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    fn test_pit2() {
        let kvm = Kvm::new().unwrap();
        let vm = kvm.create_vm().unwrap();
        assert!(vm.create_pit2(kvm_pit_config::default()).is_ok());

        let pit2 = vm.get_pit2().unwrap();
        vm.set_pit2(&pit2).unwrap();
        let mut other_pit2 = vm.get_pit2().unwrap();
        // Load time will differ, let's overwrite it so we can test equality.
        other_pit2.channels[0].count_load_time = pit2.channels[0].count_load_time;
        other_pit2.channels[1].count_load_time = pit2.channels[1].count_load_time;
        other_pit2.channels[2].count_load_time = pit2.channels[2].count_load_time;
        assert_eq!(pit2, other_pit2);
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[test]
    fn test_clock() {
        let kvm = Kvm::new().unwrap();
        let vm = kvm.create_vm().unwrap();

        // Get current time.
        let orig = vm.get_clock().unwrap();

        // Reset time.
        let mut fudged = kvm_clock_data::default();
        fudged.clock = 10;
        vm.set_clock(&fudged).unwrap();

        // Get new time.
        let new = vm.get_clock().unwrap();

        // Verify new time has progressed but is smaller than orig time.
        assert!(fudged.clock < new.clock);
        assert!(new.clock < orig.clock);
    }

    #[test]
    fn test_register_ioevent() {
        assert_eq!(std::mem::size_of::<NoDatamatch>(), 0);

        let kvm = Kvm::new().unwrap();
        let vm_fd = kvm.create_vm().unwrap();
        let evtfd = EventFd::new(EFD_NONBLOCK).unwrap();
        assert!(vm_fd
            .register_ioevent(&evtfd, &IoEventAddress::Pio(0xf4), NoDatamatch)
            .is_ok());
        assert!(vm_fd
            .register_ioevent(&evtfd, &IoEventAddress::Mmio(0x1000), NoDatamatch)
            .is_ok());
        assert!(vm_fd
            .register_ioevent(&evtfd, &IoEventAddress::Pio(0xc1), 0x7fu8)
            .is_ok());
        assert!(vm_fd
            .register_ioevent(&evtfd, &IoEventAddress::Pio(0xc2), 0x1337u16)
            .is_ok());
        assert!(vm_fd
            .register_ioevent(&evtfd, &IoEventAddress::Pio(0xc4), 0xdead_beefu32)
            .is_ok());
        assert!(vm_fd
            .register_ioevent(&evtfd, &IoEventAddress::Pio(0xc8), 0xdead_beef_dead_beefu64)
            .is_ok());
    }

    #[test]
    fn test_unregister_ioevent() {
        assert_eq!(std::mem::size_of::<NoDatamatch>(), 0);

        let kvm = Kvm::new().unwrap();
        let vm_fd = kvm.create_vm().unwrap();
        let evtfd = EventFd::new(EFD_NONBLOCK).unwrap();
        let pio_addr = IoEventAddress::Pio(0xf4);
        let mmio_addr = IoEventAddress::Mmio(0x1000);

        // First try to unregister addresses which have not been registered.
        assert!(vm_fd.unregister_ioevent(&evtfd, &pio_addr).is_err());
        assert!(vm_fd.unregister_ioevent(&evtfd, &mmio_addr).is_err());

        // Now register the addresses
        assert!(vm_fd
            .register_ioevent(&evtfd, &pio_addr, NoDatamatch)
            .is_ok());
        assert!(vm_fd
            .register_ioevent(&evtfd, &mmio_addr, NoDatamatch)
            .is_ok());

        // Try again unregistering the addresses. This time it should work
        // since they have been previously registered.
        assert!(vm_fd.unregister_ioevent(&evtfd, &pio_addr).is_ok());
        assert!(vm_fd.unregister_ioevent(&evtfd, &mmio_addr).is_ok());
    }

    #[test]
    #[cfg(any(
        target_arch = "x86",
        target_arch = "x86_64",
        target_arch = "arm",
        target_arch = "aarch64"
    ))]
    fn test_register_irqfd() {
        let kvm = Kvm::new().unwrap();
        let vm_fd = kvm.create_vm().unwrap();
        let evtfd1 = EventFd::new(EFD_NONBLOCK).unwrap();
        let evtfd2 = EventFd::new(EFD_NONBLOCK).unwrap();
        let evtfd3 = EventFd::new(EFD_NONBLOCK).unwrap();
        if cfg!(any(target_arch = "x86", target_arch = "x86_64")) {
            assert!(vm_fd.create_irq_chip().is_ok());
            assert!(vm_fd.register_irqfd(&evtfd1, 4).is_ok());
            assert!(vm_fd.register_irqfd(&evtfd2, 8).is_ok());
            assert!(vm_fd.register_irqfd(&evtfd3, 4).is_ok());
        }

        // On aarch64, this fails because setting up the interrupt controller is mandatory before
        // registering any IRQ.
        // On x86_64 this fails as the event fd was already matched with a GSI.
        assert!(vm_fd.register_irqfd(&evtfd3, 4).is_err());
        assert!(vm_fd.register_irqfd(&evtfd3, 5).is_err());
    }

    #[test]
    #[cfg(any(
        target_arch = "x86",
        target_arch = "x86_64",
        target_arch = "arm",
        target_arch = "aarch64"
    ))]
    fn test_unregister_irqfd() {
        let kvm = Kvm::new().unwrap();
        let vm_fd = kvm.create_vm().unwrap();
        let evtfd1 = EventFd::new(EFD_NONBLOCK).unwrap();
        let evtfd2 = EventFd::new(EFD_NONBLOCK).unwrap();
        let evtfd3 = EventFd::new(EFD_NONBLOCK).unwrap();
        if cfg!(any(target_arch = "x86", target_arch = "x86_64")) {
            assert!(vm_fd.create_irq_chip().is_ok());
            assert!(vm_fd.register_irqfd(&evtfd1, 4).is_ok());
            assert!(vm_fd.register_irqfd(&evtfd2, 8).is_ok());
            assert!(vm_fd.register_irqfd(&evtfd3, 4).is_ok());
            assert!(vm_fd.unregister_irqfd(&evtfd2, 8).is_ok());
            // KVM irqfd doesn't report failure on this case:(
            assert!(vm_fd.unregister_irqfd(&evtfd2, 8).is_ok());
        }

        // Duplicated eventfd registration.
        // On aarch64, this fails because setting up the interrupt controller is mandatory before
        // registering any IRQ.
        // On x86_64 this fails as the event fd was already matched with a GSI.
        assert!(vm_fd.register_irqfd(&evtfd3, 4).is_err());
        assert!(vm_fd.register_irqfd(&evtfd3, 5).is_err());

        // KVM irqfd doesn't report failure on this case:(
        assert!(vm_fd.unregister_irqfd(&evtfd3, 5).is_ok());
    }

    #[test]
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    fn test_faulty_vm_fd() {
        let badf_errno = libc::EBADF;

        let faulty_vm_fd = VmFd {
            vm: unsafe { File::from_raw_fd(-1) },
            run_size: 0,
        };

        let invalid_mem_region = kvm_userspace_memory_region {
            slot: 0,
            guest_phys_addr: 0,
            memory_size: 0,
            userspace_addr: 0,
            flags: 0,
        };

        assert_eq!(
            unsafe {
                faulty_vm_fd
                    .set_user_memory_region(invalid_mem_region)
                    .unwrap_err()
                    .errno()
            },
            badf_errno
        );
        assert_eq!(
            faulty_vm_fd.set_tss_address(0).unwrap_err().errno(),
            badf_errno
        );
        assert_eq!(
            faulty_vm_fd.create_irq_chip().unwrap_err().errno(),
            badf_errno
        );
        assert_eq!(
            faulty_vm_fd
                .create_pit2(kvm_pit_config::default())
                .unwrap_err()
                .errno(),
            badf_errno
        );
        let event_fd = EventFd::new(EFD_NONBLOCK).unwrap();
        assert_eq!(
            faulty_vm_fd
                .register_ioevent(&event_fd, &IoEventAddress::Pio(0), 0u64)
                .unwrap_err()
                .errno(),
            badf_errno
        );
        assert_eq!(
            faulty_vm_fd
                .get_irqchip(&mut kvm_irqchip::default())
                .unwrap_err()
                .errno(),
            badf_errno
        );
        assert_eq!(
            faulty_vm_fd
                .set_irqchip(&kvm_irqchip::default())
                .unwrap_err()
                .errno(),
            badf_errno
        );
        assert_eq!(faulty_vm_fd.get_clock().unwrap_err().errno(), badf_errno);
        assert_eq!(
            faulty_vm_fd
                .set_clock(&kvm_clock_data::default())
                .unwrap_err()
                .errno(),
            badf_errno
        );
        assert_eq!(faulty_vm_fd.get_pit2().unwrap_err().errno(), badf_errno);
        assert_eq!(
            faulty_vm_fd
                .set_pit2(&kvm_pit_state2::default())
                .unwrap_err()
                .errno(),
            badf_errno
        );
        assert_eq!(
            faulty_vm_fd
                .register_irqfd(&event_fd, 0)
                .unwrap_err()
                .errno(),
            badf_errno
        );

        assert_eq!(
            faulty_vm_fd.create_vcpu(0).err().unwrap().errno(),
            badf_errno
        );

        assert_eq!(
            faulty_vm_fd.get_dirty_log(0, 0).unwrap_err().errno(),
            badf_errno
        );
    }

    #[test]
    #[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
    fn test_get_preferred_target() {
        let kvm = Kvm::new().unwrap();
        let vm = kvm.create_vm().unwrap();
        let mut kvi: kvm_bindings::kvm_vcpu_init = kvm_bindings::kvm_vcpu_init::default();
        assert!(vm.get_preferred_target(&mut kvi).is_ok());
    }

    /// As explained in the example code related to signal_msi(), sending
    /// a random MSI vector will always fail because no vector has been
    /// previously allocated from the guest itself.
    #[test]
    #[cfg(any(
        target_arch = "x86",
        target_arch = "x86_64",
        target_arch = "arm",
        target_arch = "aarch64"
    ))]
    fn test_signal_msi_failure() {
        let kvm = Kvm::new().unwrap();
        let vm = kvm.create_vm().unwrap();
        let msi = kvm_msi::default();
        assert!(vm.signal_msi(msi).is_err());
    }

    #[test]
    #[cfg(not(any(target_arch = "arm", target_arch = "aarch64")))]
    fn test_enable_cap_failure() {
        let kvm = Kvm::new().unwrap();
        let vm = kvm.create_vm().unwrap();
        let cap: kvm_enable_cap = Default::default();
        // Providing the `kvm_enable_cap` structure filled with default() should
        // always result in a failure as it is not a valid capability.
        assert!(vm.enable_cap(&cap).is_err());
    }

    #[test]
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    fn test_enable_split_irqchip_cap() {
        let kvm = Kvm::new().unwrap();
        let vm = kvm.create_vm().unwrap();
        let mut cap: kvm_enable_cap = Default::default();
        cap.cap = KVM_CAP_SPLIT_IRQCHIP;
        // As per the KVM documentation, KVM_CAP_SPLIT_IRQCHIP only emulates
        // the local APIC in kernel, expecting that a userspace IOAPIC will
        // be implemented by the VMM.
        // Along with this capability, the user needs to specify the number
        // of pins reserved for the userspace IOAPIC. This number needs to be
        // provided through the first argument of the capability structure, as
        // specified in KVM documentation:
        //     args[0] - number of routes reserved for userspace IOAPICs
        //
        // Because an IOAPIC supports 24 pins, that's the reason why this test
        // picked this number as reference.
        cap.args[0] = 24;
        assert!(vm.enable_cap(&cap).is_ok());
    }

    #[test]
    fn test_set_gsi_routing() {
        let kvm = Kvm::new().unwrap();
        let vm = kvm.create_vm().unwrap();
        if cfg!(target_arch = "x86") || cfg!(target_arch = "x86_64") {
            let irq_routing = kvm_irq_routing::default();
            // Expect failure for x86 since the irqchip is not created yet.
            assert!(vm.set_gsi_routing(&irq_routing).is_err());
            vm.create_irq_chip().unwrap();
        }
        let irq_routing = kvm_irq_routing::default();
        assert!(vm.set_gsi_routing(&irq_routing).is_ok());
    }
}
