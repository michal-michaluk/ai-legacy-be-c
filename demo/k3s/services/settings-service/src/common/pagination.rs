// src/common/pagination.rs — shared value object (same shape, one home).
//
// Synchronous, self-validating. 1-based page, size capped (C3/limits). Passed
// by reference through the repository port so adapters compute LIMIT/OFFSET.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaginationError {
    BadPage,
    BadSize,
}

impl fmt::Display for PaginationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PaginationError::BadPage => write!(f, "page must be >= 1"),
            PaginationError::BadSize => write!(f, "size must be within 1..=100"),
        }
    }
}

/// Real pagination, never truncation. `page` is 1-based; `size` is 1..=100.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pagination {
    page: u32,
    size: u32,
}

impl Pagination {
    /// Build a pagination request, validating the bounds. Errors map to a **400**
    /// at the presentation layer — never a 500 (A8).
    pub fn new(page: u32, size: u32) -> Result<Self, PaginationError> {
        if page < 1 {
            return Err(PaginationError::BadPage);
        }
        if !(1..=100).contains(&size) {
            return Err(PaginationError::BadSize);
        }
        Ok(Self { page, size })
    }

    /// Default pagination used when the client omits the query params.
    pub fn default_page() -> Self {
        Self { page: 1, size: 20 }
    }

    pub fn page(&self) -> u32 {
        self.page
    }
    pub fn size(&self) -> u32 {
        self.size
    }
    pub fn offset(&self) -> u64 {
        (self.page as u64 - 1) * self.size as u64
    }
    pub fn limit(&self) -> u32 {
        self.size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_page_is_rejected() {
        assert_eq!(Pagination::new(0, 10), Err(PaginationError::BadPage));
    }

    #[test]
    fn size_above_cap_is_rejected() {
        assert_eq!(Pagination::new(1, 101), Err(PaginationError::BadSize));
        assert_eq!(Pagination::new(1, 0), Err(PaginationError::BadSize));
    }

    #[test]
    fn valid_bounds_are_accepted() {
        let p = Pagination::new(2, 50).unwrap();
        assert_eq!(p.offset(), 50);
        assert_eq!(p.limit(), 50);
        assert_eq!(p.page(), 2);
        assert_eq!(p.size(), 50);
    }

    #[test]
    fn first_page_offsets_to_zero() {
        assert_eq!(Pagination::new(1, 20).unwrap().offset(), 0);
    }

    #[test]
    fn error_messages_match_stable_forms() {
        assert_eq!(PaginationError::BadPage.to_string(), "page must be >= 1");
        assert_eq!(
            PaginationError::BadSize.to_string(),
            "size must be within 1..=100"
        );
    }

    #[test]
    fn default_is_first_page() {
        let p = Pagination::default_page();
        assert_eq!(p.page(), 1);
        assert_eq!(p.size(), 20);
    }
}
