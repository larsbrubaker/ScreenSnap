//! SCAFFOLD — replaced by the drawn app / menu-bar icon (see docs/PLAN.md §Native).

/// A `size`×`size` RGBA8 icon.
pub fn rgba(size: u32) -> Vec<u8> {
    vec![0; (size * size * 4) as usize]
}
