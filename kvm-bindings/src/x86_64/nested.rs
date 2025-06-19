//! Higher-level abstractions for working with nested state.
//!
//! Getting and setting the nested KVM state is helpful if nested virtualization
//! is used and the state needs to be serialized, e.g., for live-migration or
//! state save/resume. See [`KvmNestedStateBuffer`].

use crate::KVM_STATE_NESTED_SVM_VMCB_SIZE;
use crate::{KVM_STATE_NESTED_VMX_VMCS_SIZE, kvm_nested_state__bindgen_ty_1};
use core::mem;

/// Non-zero variant of the bindgen data union.
///
/// Please note that on SVM, this type wastes one page as the VMX state is
/// larger.
#[derive(Clone, Copy)]
#[cfg_attr(feature = "serde", derive(zerocopy::Immutable, zerocopy::FromBytes))]
#[repr(C)]
pub union kvm_nested_state__data {
    pub vmx: kvm_vmx_nested_state_data,
    pub svm: kvm_svm_nested_state_data,
}

impl Default for kvm_nested_state__data {
    fn default() -> Self {
        // SAFETY: Every bit pattern is valid.
        unsafe { mem::zeroed() }
    }
}

#[derive(Clone, Copy)]
#[cfg_attr(
    feature = "serde",
    derive(zerocopy::IntoBytes, zerocopy::Immutable, zerocopy::FromBytes)
)]
#[repr(C)]
pub struct kvm_vmx_nested_state_data {
    pub vmcs12: [u8; KVM_STATE_NESTED_VMX_VMCS_SIZE as usize],
    pub shadow_vmcs12: [u8; KVM_STATE_NESTED_VMX_VMCS_SIZE as usize],
}

#[derive(Clone, Copy)]
#[cfg_attr(
    feature = "serde",
    derive(zerocopy::IntoBytes, zerocopy::Immutable, zerocopy::FromBytes)
)]
#[repr(C)]
pub struct kvm_svm_nested_state_data {
    pub vmcb12: [u8; KVM_STATE_NESTED_SVM_VMCB_SIZE as usize],
}

/// A stack-allocated buffer for nested KVM state including the mandatory
/// header with meta-information.
///
/// KVM uses a dynamically sized buffer structure (with a header reporting the
/// size of the buffer/state). This helper type makes working with
/// `get_nested_state()` and `set_nested_state`() significantly more convenient
/// at the cost of a slightly higher memory footprint in some cases.
///
/// # Type Size
///
/// On Intel VMX, the actual state requires `128 + 8192 == 8320` bytes, on
/// AMD SVM, the actual state requires `128 + 4096 == 4224` bytes. This type
/// doesn't make a differentiation and unifies the required memory. By
/// sacrificing a few more bytes on VMX, this type is generally convenient to
/// use.
#[derive(Clone, Copy)]
#[cfg_attr(
    feature = "serde",
    derive(zerocopy::IntoBytes, zerocopy::Immutable, zerocopy::FromBytes)
)]
#[repr(C)]
#[non_exhaustive] // Prevent constructor bypass in public API.
pub struct KvmNestedStateBuffer {
    pub flags: u16,
    pub format: u16,
    pub size: u32,
    pub hdr: kvm_nested_state__bindgen_ty_1,
    pub data: kvm_nested_state__data,
}

impl KvmNestedStateBuffer {
    /// Creates a new empty buffer, ready for nested state to be stored in by KVM.
    ///
    /// The `size` property will report the size of the buffer to KVM.
    pub fn empty() -> Self {
        // SAFETY: Every bit pattern is valid.
        let mut this: KvmNestedStateBuffer = unsafe { mem::zeroed() };
        // This way, KVM knows the size of the buffer to store state into.
        // See: https://elixir.bootlin.com/linux/v6.12/source/arch/x86/kvm/x86.c#L6193
        this.size = size_of::<Self>() as u32;
        this
    }
}

impl Default for KvmNestedStateBuffer {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::kvm_nested_state as kvm_nested_state_raw_binding;

    #[test]
    fn test_layout() {
        assert_eq!(
            align_of::<kvm_nested_state_raw_binding>(),
            align_of::<KvmNestedStateBuffer>()
        );
        assert!(size_of::<KvmNestedStateBuffer>() > size_of::<kvm_nested_state_raw_binding>());
        // When this fails/changes, we should re-evaluate the overall types and API
        assert_eq!(size_of::<KvmNestedStateBuffer>(), 8320);
    }
}
