// Copyright 2024 © Institute of Software, CAS. All rights reserved.
// Copyright 2019 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

#[allow(clippy::all)]
#[allow(clippy::undocumented_unsafe_blocks)]
pub mod bindings;
#[cfg(feature = "fam-wrappers")]
pub mod fam_wrappers;

#[cfg(feature = "serde")]
mod serialize;

pub use self::bindings::*;
pub const KVM_COALESCED_MMIO_PAGE_OFFSET: u32 = 1;
#[cfg(feature = "fam-wrappers")]
pub use self::fam_wrappers::*;
