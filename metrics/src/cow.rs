use crate::label::Label;
use alloc::borrow::Borrow;
use alloc::string::String;
use alloc::vec::Vec;
use core::cmp::Ordering;
use core::fmt;
use core::hash::{Hash, Hasher};
use core::marker::PhantomData;
use core::mem::ManuallyDrop;
use core::ptr::{slice_from_raw_parts, NonNull};

/// A clone-on-write smart pointer with an optimized memory layout.
pub struct Cow<'a, T: Cowable + ?Sized + 'a> {
    /// Pointer to data.
    ptr: NonNull<T::Pointer>,

    /// Pointer metadata: length and capacity.
    meta: Metadata,

    /// Lifetime marker.
    marker: PhantomData<&'a T>,
}

impl<T> Cow<'_, T>
where
    T: Cowable + ?Sized,
{
    #[inline]
    pub fn owned(val: T::Owned) -> Self {
        let (ptr, meta) = T::owned_into_parts(val);

        Cow {
            ptr,
            meta,
            marker: PhantomData,
        }
    }
}

impl<'a, T> Cow<'a, T>
where
    T: Cowable + ?Sized,
{
    #[inline]
    pub fn borrowed(val: &'a T) -> Self {
        let (ptr, meta) = T::ref_into_parts(val);

        Cow {
            ptr,
            meta,
            marker: PhantomData,
        }
    }

    #[inline]
    pub fn into_owned(self) -> T::Owned {
        let cow = ManuallyDrop::new(self);

        if cow.is_borrowed() {
            unsafe { T::clone_from_parts(cow.ptr, &cow.meta) }
        } else {
            unsafe { T::owned_from_parts(cow.ptr, &cow.meta) }
        }
    }

    #[inline]
    pub fn is_borrowed(&self) -> bool {
        self.meta.capacity() == 0
    }

    #[inline]
    pub fn is_owned(&self) -> bool {
        self.meta.capacity() != 0
    }

    #[inline]
    fn borrow(&self) -> &T {
        unsafe { &*T::ref_from_parts(self.ptr, &self.meta) }
    }
}

// Implementations of constant functions for creating `Cow` via static strings, static string
// slices, and static label slices.
impl<'a> Cow<'a, str> {
    pub const fn const_str(val: &'a str) -> Self {
        Cow {
            // We are casting *const T to *mut T, however for all borrowed values
            // this raw pointer is only ever dereferenced back to &T.
            ptr: unsafe { NonNull::new_unchecked(val.as_ptr() as *mut u8) },
            meta: Metadata::from_ref(val.len()),
            marker: PhantomData,
        }
    }
}

impl<'a> Cow<'a, [Cow<'static, str>]> {
    pub const fn const_slice(val: &'a [Cow<'static, str>]) -> Self {
        Cow {
            ptr: unsafe { NonNull::new_unchecked(val.as_ptr() as *mut Cow<'static, str>) },
            meta: Metadata::from_ref(val.len()),
            marker: PhantomData,
        }
    }
}

impl<'a> Cow<'a, [Label]> {
    pub const fn const_slice(val: &'a [Label]) -> Self {
        Cow {
            ptr: unsafe { NonNull::new_unchecked(val.as_ptr() as *mut Label) },
            meta: Metadata::from_ref(val.len()),
            marker: PhantomData,
        }
    }
}

impl<T> Hash for Cow<'_, T>
where
    T: Hash + Cowable + ?Sized,
{
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.borrow().hash(state)
    }
}

impl<'a, T> Default for Cow<'a, T>
where
    T: Cowable + ?Sized,
    &'a T: Default,
{
    #[inline]
    fn default() -> Self {
        Cow::borrowed(Default::default())
    }
}

impl<T> Eq for Cow<'_, T> where T: Eq + Cowable + ?Sized {}

impl<A, B> PartialOrd<Cow<'_, B>> for Cow<'_, A>
where
    A: Cowable + ?Sized + PartialOrd<B>,
    B: Cowable + ?Sized,
{
    #[inline]
    fn partial_cmp(&self, other: &Cow<'_, B>) -> Option<Ordering> {
        PartialOrd::partial_cmp(self.borrow(), other.borrow())
    }
}

impl<T> Ord for Cow<'_, T>
where
    T: Ord + Cowable + ?Sized,
{
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        Ord::cmp(self.borrow(), other.borrow())
    }
}

impl<'a, T> From<&'a T> for Cow<'a, T>
where
    T: Cowable + ?Sized,
{
    #[inline]
    fn from(val: &'a T) -> Self {
        Cow::borrowed(val)
    }
}

impl From<std::borrow::Cow<'static, str>> for Cow<'_, str> {
    #[inline]
    fn from(s: std::borrow::Cow<'static, str>) -> Self {
        match s {
            std::borrow::Cow::Borrowed(bs) => Cow::borrowed(bs),
            std::borrow::Cow::Owned(os) => Cow::owned(os),
        }
    }
}

impl From<String> for Cow<'_, str> {
    #[inline]
    fn from(s: String) -> Self {
        Cow::owned(s)
    }
}

impl From<Vec<Label>> for Cow<'_, [Label]> {
    #[inline]
    fn from(v: Vec<Label>) -> Self {
        Cow::owned(v)
    }
}

impl From<Vec<Cow<'static, str>>> for Cow<'_, [Cow<'static, str>]> {
    #[inline]
    fn from(v: Vec<Cow<'static, str>>) -> Self {
        Cow::owned(v)
    }
}

impl<T> Drop for Cow<'_, T>
where
    T: Cowable + ?Sized,
{
    #[inline]
    fn drop(&mut self) {
        if self.is_owned() {
            unsafe { T::owned_from_parts(self.ptr, &self.meta) };
        }
    }
}

impl<'a, T> Clone for Cow<'a, T>
where
    T: Cowable + ?Sized,
{
    #[inline]
    fn clone(&self) -> Self {
        if self.is_owned() {
            // Gotta clone the actual inner value.
            Cow::owned(unsafe { T::clone_from_parts(self.ptr, &self.meta) })
        } else {
            Cow { ..*self }
        }
    }
}

impl<T> core::ops::Deref for Cow<'_, T>
where
    T: Cowable + ?Sized,
{
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        self.borrow()
    }
}

impl<T> AsRef<T> for Cow<'_, T>
where
    T: Cowable + ?Sized,
{
    #[inline]
    fn as_ref(&self) -> &T {
        self.borrow()
    }
}

impl<T> Borrow<T> for Cow<'_, T>
where
    T: Cowable + ?Sized,
{
    #[inline]
    fn borrow(&self) -> &T {
        self.borrow()
    }
}

impl<A, B> PartialEq<Cow<'_, B>> for Cow<'_, A>
where
    A: Cowable + ?Sized,
    B: Cowable + ?Sized,
    A: PartialEq<B>,
{
    fn eq(&self, other: &Cow<B>) -> bool {
        self.borrow() == other.borrow()
    }
}

impl<T> fmt::Debug for Cow<'_, T>
where
    T: Cowable + fmt::Debug + ?Sized,
{
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.borrow().fmt(f)
    }
}

impl<T> fmt::Display for Cow<'_, T>
where
    T: Cowable + fmt::Display + ?Sized,
{
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.borrow().fmt(f)
    }
}

unsafe impl<T: Cowable + Sync + ?Sized> Sync for Cow<'_, T> {}
unsafe impl<T: Cowable + Send + ?Sized> Send for Cow<'_, T> {}

/// Helper trait required by `Cow<T>` to extract capacity of owned
/// variant of `T`, and manage conversions.
///
/// This can be only implemented on types that match requirements:
///
/// + `T::Owned` has a `capacity`, which is an extra word that is absent in `T`.
/// + `T::Owned` with `capacity` of `0` does not allocate memory.
/// + `T::Owned` can be reconstructed from `*mut T` borrowed out of it, plus capacity.
pub unsafe trait Cowable {
    type Pointer;
    type Owned;

    fn ref_into_parts(&self) -> (NonNull<Self::Pointer>, Metadata);
    fn owned_into_parts(owned: Self::Owned) -> (NonNull<Self::Pointer>, Metadata);

    unsafe fn ref_from_parts(ptr: NonNull<Self::Pointer>, metadata: &Metadata) -> *const Self;
    unsafe fn owned_from_parts(ptr: NonNull<Self::Pointer>, metadata: &Metadata) -> Self::Owned;
    unsafe fn clone_from_parts(ptr: NonNull<Self::Pointer>, metadata: &Metadata) -> Self::Owned;
}

unsafe impl Cowable for str {
    type Pointer = u8;
    type Owned = String;

    #[inline]
    fn ref_into_parts(&self) -> (NonNull<u8>, Metadata) {
        // A note on soundness:
        //
        // We are casting *const T to *mut T, however for all borrowed values
        // this raw pointer is only ever dereferenced back to &T.
        let ptr = unsafe { NonNull::new_unchecked(self.as_ptr() as *mut _) };
        let metadata = Metadata::from_ref(self.len());
        (ptr, metadata)
    }

    #[inline]
    unsafe fn ref_from_parts(ptr: NonNull<u8>, metadata: &Metadata) -> *const str {
        slice_from_raw_parts(ptr.as_ptr(), metadata.length()) as *const _
    }

    #[inline]
    fn owned_into_parts(owned: String) -> (NonNull<u8>, Metadata) {
        let mut owned = ManuallyDrop::new(owned);
        let ptr = unsafe { NonNull::new_unchecked(owned.as_mut_ptr()) };
        let metadata = Metadata::from_owned(owned.len(), owned.capacity());
        (ptr, metadata)
    }

    #[inline]
    unsafe fn owned_from_parts(ptr: NonNull<u8>, metadata: &Metadata) -> String {
        String::from_utf8_unchecked(Vec::from_raw_parts(
            ptr.as_ptr(),
            metadata.length(),
            metadata.capacity(),
        ))
    }

    #[inline]
    unsafe fn clone_from_parts(ptr: NonNull<u8>, metadata: &Metadata) -> Self::Owned {
        let str = Self::ref_from_parts(ptr, metadata);
        str.as_ref().unwrap().to_string()
    }
}

unsafe impl<'a> Cowable for [Cow<'a, str>] {
    type Pointer = Cow<'a, str>;
    type Owned = Vec<Cow<'a, str>>;

    #[inline]
    fn ref_into_parts(&self) -> (NonNull<Cow<'a, str>>, Metadata) {
        // A note on soundness:
        //
        // We are casting *const T to *mut T, however for all borrowed values
        // this raw pointer is only ever dereferenced back to &T.
        let ptr = unsafe { NonNull::new_unchecked(self.as_ptr() as *mut _) };
        let metadata = Metadata::from_ref(self.len());
        (ptr, metadata)
    }

    #[inline]
    unsafe fn ref_from_parts(
        ptr: NonNull<Cow<'a, str>>,
        metadata: &Metadata,
    ) -> *const [Cow<'a, str>] {
        slice_from_raw_parts(ptr.as_ptr(), metadata.length())
    }

    #[inline]
    fn owned_into_parts(owned: Vec<Cow<'a, str>>) -> (NonNull<Cow<'a, str>>, Metadata) {
        let mut owned = ManuallyDrop::new(owned);
        let ptr = unsafe { NonNull::new_unchecked(owned.as_mut_ptr()) };
        let metadata = Metadata::from_owned(owned.len(), owned.capacity());
        (ptr, metadata)
    }

    #[inline]
    unsafe fn owned_from_parts(
        ptr: NonNull<Cow<'a, str>>,
        metadata: &Metadata,
    ) -> Vec<Cow<'a, str>> {
        Vec::from_raw_parts(ptr.as_ptr(), metadata.length(), metadata.capacity())
    }

    #[inline]
    unsafe fn clone_from_parts(ptr: NonNull<Cow<'a, str>>, metadata: &Metadata) -> Self::Owned {
        let ptr = Self::ref_from_parts(ptr, metadata);
        let xs = ptr.as_ref().unwrap();
        let mut owned = Vec::with_capacity(xs.len() + 1);
        owned.extend_from_slice(xs);
        owned
    }
}

unsafe impl Cowable for [Label] {
    type Pointer = Label;
    type Owned = Vec<Label>;

    #[inline]
    fn ref_into_parts(&self) -> (NonNull<Label>, Metadata) {
        // A note on soundness:
        //
        // We are casting *const T to *mut T, however for all borrowed values
        // this raw pointer is only ever dereferenced back to &T.
        let ptr = unsafe { NonNull::new_unchecked(self.as_ptr() as *mut _) };
        let metadata = Metadata::from_ref(self.len());
        (ptr, metadata)
    }

    #[inline]
    unsafe fn ref_from_parts(ptr: NonNull<Label>, metadata: &Metadata) -> *const [Label] {
        slice_from_raw_parts(ptr.as_ptr(), metadata.length())
    }

    #[inline]
    fn owned_into_parts(owned: Vec<Label>) -> (NonNull<Label>, Metadata) {
        let mut owned = ManuallyDrop::new(owned);
        let ptr = unsafe { NonNull::new_unchecked(owned.as_mut_ptr()) };
        let metadata = Metadata::from_owned(owned.len(), owned.capacity());
        (ptr, metadata)
    }

    #[inline]
    unsafe fn owned_from_parts(ptr: NonNull<Label>, metadata: &Metadata) -> Vec<Label> {
        Vec::from_raw_parts(ptr.as_ptr(), metadata.length(), metadata.capacity())
    }

    #[inline]
    unsafe fn clone_from_parts(ptr: NonNull<Label>, metadata: &Metadata) -> Self::Owned {
        let xs = Self::ref_from_parts(ptr, metadata);
        xs.as_ref().unwrap().to_vec()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Metadata(usize, usize);

impl Metadata {
    #[inline]
    fn length(&self) -> usize {
        self.0
    }

    #[inline]
    fn capacity(&self) -> usize {
        self.1
    }

    pub const fn from_ref(len: usize) -> Metadata {
        Metadata(len, 0)
    }

    pub const fn from_owned(len: usize, capacity: usize) -> Metadata {
        Metadata(len, capacity)
    }

    pub const fn borrowed() -> Metadata {
        Metadata(0, 0)
    }

    pub const fn owned() -> Metadata {
        Metadata(0, 1)
    }
}

/*

This can be enabled again when we have a way to do panics/asserts in stable Rust,
since const panicking is behind a feature flag at the moment.

const MASK_LO: usize = u32::MAX as usize;
const MASK_HI: usize = !MASK_LO;

#[cfg(target_pointer_width = "64")]
impl Metadata {
    #[inline]
    fn length(&self) -> usize {
        self.0 & MASK_LO
    }

    #[inline]
    fn capacity(&self) -> usize {
        self.0 & MASK_HI
    }

    pub const fn from_ref(len: usize) -> Metadata {
        if len & MASK_HI != 0 {
            panic!("Cow: length out of bounds for referenced value");
        }

        Metadata(len)
    }

    pub const fn from_owned(len: usize, capacity: usize) -> Metadata {
        if len & MASK_HI != 0 {
            panic!("Cow: length out of bounds for owned value");
        }

        if capacity & MASK_HI != 0 {
            panic!("Cow: capacity out of bounds for owned value");
        }

        Metadata((capacity & MASK_LO) << 32 | len & MASK_LO)
    }

    pub const fn borrowed() -> Metadata {
        Metadata(0)
    }

    pub const fn owned() -> Metadata {
        Metadata(1 << 32)
    }
}*/
