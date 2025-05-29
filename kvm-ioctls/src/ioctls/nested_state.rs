// Copyright © 2025 Cyberus Technology GmbH
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Module for working with nested KVM state.
//!
//! Getting and setting the nested KVM state is helpful if nested virtualization
//! is used and the state needs to be serialized, e.g., for live-migration or
//! state save/resume. The main export is [`KvmNestedStateBuffer`].

use kvm_bindings::{kvm_nested_state, kvm_svm_nested_state_data, kvm_vmx_nested_state_data};
use std::fmt::{Debug, Formatter};
use std::{cmp, mem};

/// Helper type for [`KvmNestedStateBuffer`] unifying the actual state according
/// to KVM bindings.
///
/// Please note that on SVM, this type wastes one page as the VMX state is
/// larger.
#[cfg(target_arch = "x86_64")]
#[repr(C)]
#[derive(Copy, Clone)]
pub(crate) union KvmNestedStateData {
    pub vmx_state: kvm_vmx_nested_state_data,
    pub svm_state: kvm_svm_nested_state_data,
}

impl Debug for KvmNestedStateData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "KvmNestedStateData({} bytes)", size_of::<Self>())
    }
}

/// A stack-allocated buffer for nested KVM state.
///
/// KVM uses a dynamically sized buffer structure (with a header reporting the
/// size of the buffer/state) making it cumbersome to work with. This helper
/// type makes working with [`get_nested_state`] and [`set_nested_state`]
/// significantly more convenient at the cost of a slightly higher memory
/// footprint in some cases. Unlike the fixed header [`kvm_nested_state`],
/// this type acts as a stack buffer capable of holding all possible state.
///
/// The implementations for [`AsRef<[u8]>`] and [`AsMut<[u8]>`] refer to the
/// entire buffer. To only get the state size as reported by the header, use
/// [`Self::as_raw_state`].
///
/// # Type Size
///
/// On Intel VMX, the actual state requires `128 + 8192 == 8320` bytes, on
/// AMD SVM, the actual state requires `128 + 4096 == 4224` bytes. This type
/// doesn't make a differentiation and unifies the required memory. By
/// sacrificing a few more bytes on VMX, this type is more convenient to use.
///
/// [`get_nested_state`]: crate::VcpuFd::get_nested_state
/// [`set_nested_state`]: crate::VcpuFd::set_nested_state
#[repr(C)]
#[derive(Debug)]
pub struct KvmNestedStateBuffer {
    /// The fixed header from the KVM binding.
    pub(crate) header: kvm_nested_state,
    /// The actual payload.
    pub(crate) data: KvmNestedStateData,
}

impl KvmNestedStateBuffer {
    /// Creates a new type which acts as empty ready-to-use buffer for
    /// [`get_nested_state`].
    ///
    /// [`get_nested_state`]: crate::VcpuFd::get_nested_state
    pub fn new_empty() -> Self {
        // SAFETY: properly initialized `u8` values to zero
        let mut base = unsafe { mem::zeroed::<KvmNestedStateBuffer>() };
        // This is a sane default as KVM uses this field to know how many bytes
        // are allocated for it. This is crucial so that KVM actually puts the
        // state into the buffer.
        base.header.size = size_of::<Self>() as _;
        base
    }

    /// Creates a new buffer from raw state that was previously persisted using
    /// [`Self::as_raw_state`].
    ///
    /// This is useful to create a new **aligned** buffer that can be used for
    /// [`set_nested_state`]. When `raw_state` is already properly aligned,
    /// there is no advantage in using all the additional copies.
    ///
    /// [`set_nested_state`]: crate::VcpuFd::set_nested_state
    pub fn from_raw_state(raw_state: &[u8]) -> Self {
        let mut base = Self::new_empty();
        base.as_mut().copy_from_slice(raw_state);
        base
    }

    /// Returns the actual complete raw state, suited for serialization.
    ///
    /// This is useful to serialize the actual state without wasting unused
    /// buffer capacity on SVM. On VMX, this is as good as [`Self::as_ref::<[u8]>`]
    ///
    /// This is supposed to be called **after** [`Vcpu::get_nested_state`]
    /// properly stored the actual state into the buffer. Then, when the
    /// header is properly set, this method also respects the length as
    /// reported by the header, effectively not returning unused memory
    /// for VMX.
    pub fn as_actual_raw_state(&self) -> &[u8] {
        let len = self.actual_length();

        // Ensure there is at least the header.
        // It is okay if this is empty
        // -> When no nested nirtualization was used.
        assert!(len >= size_of::<kvm_nested_state>());
        // Ensure there is no invalid size
        assert!(len <= size_of::<Self>());

        // SAFETY: We checked the length and the data is initialized.
        unsafe {
            let ptr = core::ptr::addr_of!(*self);
            core::slice::from_raw_parts(ptr.cast::<u8>(), len)
        }
    }

    /// The length as reported by the header.
    pub fn actual_length(&self) -> usize {
        self.header.size as usize
    }
}

impl Clone for KvmNestedStateBuffer {
    fn clone(&self) -> Self {
        // SAFETY: Bit pattern is valid.
        let mut header_clone: kvm_nested_state = unsafe { mem::zeroed() };
        // SAFETY: header is initialized and sized
        unsafe { core::ptr::copy_nonoverlapping(&self.header, &mut header_clone, 1) };
        Self {
            header: header_clone,
            data: self.data,
        }
    }
}

#[cfg(target_arch = "x86_64")]
impl Default for KvmNestedStateBuffer {
    fn default() -> Self {
        Self::new_empty()
    }
}

impl AsRef<[u8]> for KvmNestedStateBuffer {
    fn as_ref(&self) -> &[u8] {
        let ptr = core::ptr::addr_of!(*self);
        let len = cmp::min(size_of::<Self>(), self.header.size as usize);
        // SAFETY: The reference is initialized and we checked the length.
        unsafe { core::slice::from_raw_parts(ptr.cast::<u8>(), len) }
    }
}

impl AsMut<[u8]> for KvmNestedStateBuffer {
    fn as_mut(&mut self) -> &mut [u8] {
        let ptr = core::ptr::addr_of_mut!(*self);
        let len = cmp::min(size_of::<Self>(), self.header.size as usize);
        // SAFETY: The reference is initialized and we checked the length.
        unsafe { core::slice::from_raw_parts_mut(ptr.cast::<u8>(), len) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout() {
        assert_eq!(
            align_of::<KvmNestedStateBuffer>(),
            align_of::<kvm_nested_state>()
        );
        // The type holding all payload must be bigger than just the header
        assert!(size_of::<KvmNestedStateBuffer>() > size_of::<kvm_nested_state>());
        assert_eq!(size_of::<KvmNestedStateBuffer>(), 8320);
    }

    #[test]
    fn test_usage() {
        let mut buffer = KvmNestedStateBuffer::new_empty();
        let _buffer_view = buffer.as_actual_raw_state();
        let _buffer_view = buffer.as_ref();
        let _buffer_view = buffer.as_mut();
        // By default, the header reports the full length. This tells KVM that
        // the buffer is long enough to hold the actual state.
        assert_eq!(buffer.actual_length(), size_of::<KvmNestedStateBuffer>());
    }
}
