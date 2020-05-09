// Copyright 2018 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT
//
// Portions Copyright 2017 The Chromium OS Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the THIRD-PARTY file.
use libc::{open, O_CLOEXEC, O_RDWR};
use std::fs::File;
use std::os::raw::{c_char, c_ulong};
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};

use cap::Cap;
use ioctls::vm::{new_vmfd, VmFd};
use ioctls::Result;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use kvm_bindings::{CpuId, MsrList, KVM_MAX_MSR_ENTRIES};
use kvm_ioctls::*;
use vmm_sys_util::errno;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use vmm_sys_util::ioctl::ioctl_with_mut_ptr;
use vmm_sys_util::ioctl::{ioctl, ioctl_with_val};

/// Wrapper over KVM system ioctls.
pub struct Kvm {
    kvm: File,
}

impl Kvm {
    /// Opens `/dev/kvm` and returns a `Kvm` object on success.
    ///
    /// # Example
    ///
    /// ```
    /// use kvm_ioctls::Kvm;
    /// let kvm = Kvm::new().unwrap();
    /// ```
    ///
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Result<Self> {
        // Open `/dev/kvm` using `O_CLOEXEC` flag.
        let fd = Self::open_with_cloexec(true)?;
        // Safe because we verify that ret is valid and we own the fd.
        Ok(unsafe { Self::new_with_fd_number(fd) })
    }

    /// Creates a new Kvm object assuming `fd` represents an existing open file descriptor
    /// associated with `/dev/kvm`.
    ///
    /// For usage examples check [open_with_cloexec()](struct.Kvm.html#method.open_with_cloexec).
    ///
    /// # Arguments
    ///
    /// * `fd` - File descriptor for `/dev/kvm`.
    ///
    pub unsafe fn new_with_fd_number(fd: RawFd) -> Self {
        Kvm {
            kvm: File::from_raw_fd(fd),
        }
    }

    /// Opens `/dev/kvm` and returns the fd number on success.
    ///
    /// One usecase for this method is opening `/dev/kvm` before exec-ing into a
    /// process with seccomp filters enabled that blacklist the `sys_open` syscall.
    /// For this usecase `open_with_cloexec` must be called with the `close_on_exec`
    /// parameter set to false.
    ///
    /// # Arguments
    ///
    /// * `close_on_exec`: If true opens `/dev/kvm` using the `O_CLOEXEC` flag.
    ///
    /// # Example
    ///
    /// ```
    /// # use kvm_ioctls::Kvm;
    /// let kvm_fd = Kvm::open_with_cloexec(false).unwrap();
    /// // The `kvm_fd` can now be passed to another process where we can use
    /// // `new_with_fd_number` for creating a `Kvm` object:
    /// let kvm = unsafe { Kvm::new_with_fd_number(kvm_fd) };
    /// ```
    ///
    pub fn open_with_cloexec(close_on_exec: bool) -> Result<RawFd> {
        let open_flags = O_RDWR | if close_on_exec { O_CLOEXEC } else { 0 };
        // Safe because we give a constant nul-terminated string and verify the result.
        let ret = unsafe { open("/dev/kvm\0".as_ptr() as *const c_char, open_flags) };
        if ret < 0 {
            Err(errno::Error::last())
        } else {
            Ok(ret)
        }
    }

    /// Returns the KVM API version.
    ///
    /// See the documentation for `KVM_GET_API_VERSION`.
    ///
    /// # Example
    ///
    /// ```
    /// # use kvm_ioctls::Kvm;
    /// let kvm = Kvm::new().unwrap();
    /// assert_eq!(kvm.get_api_version(), 12);
    /// ```
    ///
    pub fn get_api_version(&self) -> i32 {
        // Safe because we know that our file is a KVM fd and that the request is one of the ones
        // defined by kernel.
        unsafe { ioctl(self, KVM_GET_API_VERSION()) }
    }

    /// AArch64 specific call to get the host Intermediate Physical Address space limit.
    ///
    /// Returns 0 if the capability is not available and an integer larger than 32 otherwise.
    #[cfg(any(target_arch = "aarch64"))]
    pub fn get_host_ipa_limit(&self) -> i32 {
        self.check_extension_int(Cap::ArmVmIPASize)
    }

    /// Wrapper over `KVM_CHECK_EXTENSION`.
    ///
    /// Returns 0 if the capability is not available and a positive integer otherwise.
    fn check_extension_int(&self, c: Cap) -> i32 {
        // Safe because we know that our file is a KVM fd and that the extension is one of the ones
        // defined by kernel.
        unsafe { ioctl_with_val(self, KVM_CHECK_EXTENSION(), c as c_ulong) }
    }

    /// Checks if a particular `Cap` is available.
    ///
    /// Returns true if the capability is supported and false otherwise.
    /// See the documentation for `KVM_CHECK_EXTENSION`.
    ///
    /// # Arguments
    ///
    /// * `c` - KVM capability to check.
    ///
    /// # Example
    ///
    /// ```
    /// # use kvm_ioctls::Kvm;
    /// use kvm_ioctls::Cap;
    ///
    /// let kvm = Kvm::new().unwrap();
    /// // Check if `KVM_CAP_USER_MEMORY` is supported.
    /// assert!(kvm.check_extension(Cap::UserMemory));
    /// ```
    ///
    pub fn check_extension(&self, c: Cap) -> bool {
        self.check_extension_int(c) > 0
    }

    ///  Returns the size of the memory mapping required to use the vcpu's `kvm_run` structure.
    ///
    /// See the documentation for `KVM_GET_VCPU_MMAP_SIZE`.
    ///
    /// # Example
    ///
    /// ```
    /// # use kvm_ioctls::Kvm;
    /// let kvm = Kvm::new().unwrap();
    /// assert!(kvm.get_vcpu_mmap_size().unwrap() > 0);
    /// ```
    ///
    pub fn get_vcpu_mmap_size(&self) -> Result<usize> {
        // Safe because we know that our file is a KVM fd and we verify the return result.
        let res = unsafe { ioctl(self, KVM_GET_VCPU_MMAP_SIZE()) };
        if res > 0 {
            Ok(res as usize)
        } else {
            Err(errno::Error::last())
        }
    }

    /// Gets the recommended number of VCPUs per VM.
    ///
    /// See the documentation for `KVM_CAP_NR_VCPUS`.
    /// Default to 4 when `KVM_CAP_NR_VCPUS` is not implemented.
    ///
    /// # Example
    ///
    /// ```
    /// # use kvm_ioctls::Kvm;
    /// let kvm = Kvm::new().unwrap();
    /// // We expect the number of vCPUs to be > 0 as per KVM API documentation.
    /// assert!(kvm.get_nr_vcpus() > 0);
    /// ```
    ///
    pub fn get_nr_vcpus(&self) -> usize {
        let x = self.check_extension_int(Cap::NrVcpus);
        if x > 0 {
            x as usize
        } else {
            4
        }
    }

    /// Returns the maximum allowed memory slots per VM.
    ///
    /// KVM reports the number of available memory slots (`KVM_CAP_NR_MEMSLOTS`)
    /// using the extension interface.  Both x86 and s390 implement this, ARM
    /// and powerpc do not yet enable it.
    /// Default to 32 when `KVM_CAP_NR_MEMSLOTS` is not implemented.
    ///
    /// # Example
    ///
    /// ```
    /// # use kvm_ioctls::Kvm;
    /// let kvm = Kvm::new().unwrap();
    /// assert!(kvm.get_nr_memslots() > 0);
    /// ```
    ///
    pub fn get_nr_memslots(&self) -> usize {
        let x = self.check_extension_int(Cap::NrMemslots);
        if x > 0 {
            x as usize
        } else {
            32
        }
    }

    /// Gets the recommended maximum number of VCPUs per VM.
    ///
    /// See the documentation for `KVM_CAP_MAX_VCPUS`.
    /// Returns [get_nr_vcpus()](struct.Kvm.html#method.get_nr_vcpus) when
    /// `KVM_CAP_MAX_VCPUS` is not implemented.
    ///
    /// # Example
    ///
    /// ```
    /// # use kvm_ioctls::Kvm;
    /// let kvm = Kvm::new().unwrap();
    /// assert!(kvm.get_max_vcpus() > 0);
    /// ```
    ///
    pub fn get_max_vcpus(&self) -> usize {
        match self.check_extension_int(Cap::MaxVcpus) {
            0 => self.get_nr_vcpus(),
            x => x as usize,
        }
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    fn get_cpuid(&self, kind: u64, max_entries_count: usize) -> Result<CpuId> {
        let mut cpuid = CpuId::new(max_entries_count);

        let ret = unsafe {
            // ioctl is unsafe. The kernel is trusted not to write beyond the bounds of the memory
            // allocated for the struct. The limit is read from nent, which is set to the allocated
            // size(max_entries_count) above.
            ioctl_with_mut_ptr(self, kind, cpuid.as_mut_fam_struct_ptr())
        };
        if ret < 0 {
            return Err(errno::Error::last());
        }

        Ok(cpuid)
    }

    /// X86 specific call to get the system emulated CPUID values.
    ///
    /// See the documentation for `KVM_GET_EMULATED_CPUID`.
    ///
    /// # Arguments
    ///
    /// * `max_entries_count` - Maximum number of CPUID entries. This function can return less than
    ///                         this when the hardware does not support so many CPUID entries.
    ///
    /// # Example
    ///
    /// ```
    /// extern crate kvm_bindings;
    /// use kvm_bindings::KVM_MAX_CPUID_ENTRIES;
    /// use kvm_ioctls::Kvm;
    ///
    /// let kvm = Kvm::new().unwrap();
    /// let mut cpuid = kvm.get_emulated_cpuid(KVM_MAX_CPUID_ENTRIES).unwrap();
    /// let cpuid_entries = cpuid.as_mut_slice();
    /// assert!(cpuid_entries.len() <= KVM_MAX_CPUID_ENTRIES);
    /// ```
    ///
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    pub fn get_emulated_cpuid(&self, max_entries_count: usize) -> Result<CpuId> {
        self.get_cpuid(KVM_GET_EMULATED_CPUID(), max_entries_count)
    }

    /// X86 specific call to get the system supported CPUID values.
    ///
    /// See the documentation for `KVM_GET_SUPPORTED_CPUID`.
    ///
    /// # Arguments
    ///
    /// * `max_entries_count` - Maximum number of CPUID entries. This function can return less than
    ///                         this when the hardware does not support so many CPUID entries.
    ///
    /// # Example
    ///
    /// ```
    /// extern crate kvm_bindings;
    /// use kvm_bindings::KVM_MAX_CPUID_ENTRIES;
    /// use kvm_ioctls::Kvm;
    ///
    /// let kvm = Kvm::new().unwrap();
    /// let mut cpuid = kvm.get_emulated_cpuid(KVM_MAX_CPUID_ENTRIES).unwrap();
    /// let cpuid_entries = cpuid.as_mut_slice();
    /// assert!(cpuid_entries.len() <= KVM_MAX_CPUID_ENTRIES);
    /// ```
    ///
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    pub fn get_supported_cpuid(&self, max_entries_count: usize) -> Result<CpuId> {
        self.get_cpuid(KVM_GET_SUPPORTED_CPUID(), max_entries_count)
    }

    /// X86 specific call to get list of supported MSRS
    ///
    /// See the documentation for `KVM_GET_MSR_INDEX_LIST`.
    ///
    /// # Example
    ///
    /// ```
    /// use kvm_ioctls::Kvm;
    ///
    /// let kvm = Kvm::new().unwrap();
    /// let msr_index_list = kvm.get_msr_index_list().unwrap();
    /// ```
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    pub fn get_msr_index_list(&self) -> Result<MsrList> {
        let mut msr_list = MsrList::new(KVM_MAX_MSR_ENTRIES);

        let ret = unsafe {
            // ioctl is unsafe. The kernel is trusted not to write beyond the bounds of the memory
            // allocated for the struct. The limit is read from nmsrs, which is set to the allocated
            // size (MAX_KVM_MSR_ENTRIES) above.
            ioctl_with_mut_ptr(
                self,
                KVM_GET_MSR_INDEX_LIST(),
                msr_list.as_mut_fam_struct_ptr(),
            )
        };
        if ret < 0 {
            return Err(errno::Error::last());
        }

        // The ioctl will also update the internal `nmsrs` with the actual count.
        Ok(msr_list)
    }

    /// Creates a VM fd using the KVM fd.
    ///
    /// See the documentation for `KVM_CREATE_VM`.
    /// A call to this function will also initialize the size of the vcpu mmap area using the
    /// `KVM_GET_VCPU_MMAP_SIZE` ioctl.
    ///
    /// # Example
    ///
    /// ```
    /// # use kvm_ioctls::Kvm;
    /// let kvm = Kvm::new().unwrap();
    /// let vm = kvm.create_vm().unwrap();
    /// // Check that the VM mmap size is the same reported by `KVM_GET_VCPU_MMAP_SIZE`.
    /// assert!(vm.run_size() == kvm.get_vcpu_mmap_size().unwrap());
    /// ```
    ///
    pub fn create_vm(&self) -> Result<VmFd> {
        // Safe because we know `self.kvm` is a real KVM fd as this module is the only one that
        // create Kvm objects.
        let ret = unsafe { ioctl(&self.kvm, KVM_CREATE_VM()) };
        if ret >= 0 {
            // Safe because we verify the value of ret and we are the owners of the fd.
            let vm_file = unsafe { File::from_raw_fd(ret) };
            let run_mmap_size = self.get_vcpu_mmap_size()?;
            Ok(new_vmfd(vm_file, run_mmap_size))
        } else {
            Err(errno::Error::last())
        }
    }

    /// Creates a VmFd object from a VM RawFd.
    ///
    /// This function is unsafe as the primitives currently returned have the contract that
    /// they are the sole owner of the file descriptor they are wrapping. Usage of this function
    /// could accidentally allow violating this contract which can cause memory unsafety in code
    /// that relies on it being true.
    pub unsafe fn create_vmfd_from_rawfd(&self, fd: RawFd) -> Result<VmFd> {
        let run_mmap_size = self.get_vcpu_mmap_size()?;
        Ok(new_vmfd(File::from_raw_fd(fd), run_mmap_size))
    }
}

impl AsRawFd for Kvm {
    fn as_raw_fd(&self) -> RawFd {
        self.kvm.as_raw_fd()
    }
}

impl FromRawFd for Kvm {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        Kvm {
            kvm: File::from_raw_fd(fd),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    use kvm_bindings::KVM_MAX_CPUID_ENTRIES;
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    use vmm_sys_util::fam::FamStruct;

    #[test]
    fn test_kvm_new() {
        Kvm::new().unwrap();
    }

    #[test]
    fn test_kvm_api_version() {
        let kvm = Kvm::new().unwrap();
        assert_eq!(kvm.get_api_version(), 12);
        assert!(kvm.check_extension(Cap::UserMemory));
    }

    #[test]
    #[cfg(any(target_arch = "aarch64"))]
    fn test_get_host_ipa_limit() {
        let kvm = Kvm::new().unwrap();
        let host_ipa_limit = kvm.get_host_ipa_limit();

        if host_ipa_limit > 0 {
            assert!(host_ipa_limit >= 32);
        } else {
            // if unsupported, the return value should be 0.
            assert_eq!(host_ipa_limit, 0);
        }
    }

    #[test]
    fn test_kvm_getters() {
        let kvm = Kvm::new().unwrap();

        // vCPU related getters
        let nr_vcpus = kvm.get_nr_vcpus();
        assert!(nr_vcpus >= 4);

        assert!(kvm.get_max_vcpus() >= nr_vcpus);

        // Memory related getters
        assert!(kvm.get_vcpu_mmap_size().unwrap() > 0);
        assert!(kvm.get_nr_memslots() >= 32);
    }

    #[test]
    fn test_create_vm() {
        let kvm = Kvm::new().unwrap();
        let vm = kvm.create_vm().unwrap();

        // Test create_vmfd_from_rawfd()
        let rawfd = unsafe { libc::dup(vm.as_raw_fd()) };
        assert!(rawfd >= 0);
        let vm = unsafe { kvm.create_vmfd_from_rawfd(rawfd).unwrap() };

        assert_eq!(vm.run_size(), kvm.get_vcpu_mmap_size().unwrap());
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[test]
    fn test_get_supported_cpuid() {
        let kvm = Kvm::new().unwrap();
        let mut cpuid = kvm.get_supported_cpuid(KVM_MAX_CPUID_ENTRIES).unwrap();
        let cpuid_entries = cpuid.as_mut_slice();
        assert!(!cpuid_entries.is_empty());
        assert!(cpuid_entries.len() <= KVM_MAX_CPUID_ENTRIES);
    }

    #[test]
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    fn test_get_emulated_cpuid() {
        let kvm = Kvm::new().unwrap();
        let mut cpuid = kvm.get_emulated_cpuid(KVM_MAX_CPUID_ENTRIES).unwrap();
        let cpuid_entries = cpuid.as_mut_slice();
        assert!(!cpuid_entries.is_empty());
        assert!(cpuid_entries.len() <= KVM_MAX_CPUID_ENTRIES);
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[test]
    fn test_cpuid_clone() {
        let kvm = Kvm::new().unwrap();

        // Test from_raw_fd()
        let rawfd = unsafe { libc::dup(kvm.as_raw_fd()) };
        assert!(rawfd >= 0);
        let kvm = unsafe { Kvm::from_raw_fd(rawfd) };

        let cpuid_1 = kvm.get_supported_cpuid(KVM_MAX_CPUID_ENTRIES).unwrap();
        let cpuid_2 = CpuId::new(cpuid_1.as_fam_struct_ref().len());
        assert!(cpuid_1 != cpuid_2);
    }

    #[test]
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    fn get_msr_index_list() {
        let kvm = Kvm::new().unwrap();
        let msr_list = kvm.get_msr_index_list().unwrap();
        assert!(msr_list.as_slice().len() >= 2);
    }

    #[test]
    fn test_bad_kvm_fd() {
        let badf_errno = libc::EBADF;

        let faulty_kvm = Kvm {
            kvm: unsafe { File::from_raw_fd(-1) },
        };

        assert_eq!(
            faulty_kvm.get_vcpu_mmap_size().unwrap_err().errno(),
            badf_errno
        );
        assert_eq!(faulty_kvm.get_nr_vcpus(), 4);
        assert_eq!(faulty_kvm.get_nr_memslots(), 32);
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            assert_eq!(
                faulty_kvm.get_emulated_cpuid(4).err().unwrap().errno(),
                badf_errno
            );
            assert_eq!(
                faulty_kvm.get_supported_cpuid(4).err().unwrap().errno(),
                badf_errno
            );

            assert_eq!(
                faulty_kvm.get_msr_index_list().err().unwrap().errno(),
                badf_errno
            );
        }
        assert_eq!(faulty_kvm.create_vm().err().unwrap().errno(), badf_errno);
    }
}
