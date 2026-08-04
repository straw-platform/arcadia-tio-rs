use super::*;

unsafe extern "C" {
    /// Pops the current head commit.
    pub fn arcadia_tio_pop(handle: *mut ArcadiaTioHandle) -> c_int;
    /// Pops up to `n` current head commits.
    pub fn arcadia_tio_pop_batched(handle: *mut ArcadiaTioHandle, n: u32) -> c_int;
    /// Reverts the file to a target visible commit.
    pub fn arcadia_tio_revert_commit(
        handle: *mut ArcadiaTioHandle,
        target_commit_seq: u64,
    ) -> c_int;
    /// Reads current head commit metadata.
    pub fn arcadia_tio_head_commit(
        handle: *mut ArcadiaTioHandle,
        out_commit: *mut ArcadiaTioCommitInfo,
    ) -> c_int;
    /// Lists visible commit metadata into a native-owned commit list.
    pub fn arcadia_tio_list_commits(
        handle: *mut ArcadiaTioHandle,
        limit: u32,
        out_commits: *mut ArcadiaTioCommitList,
    ) -> c_int;
    /// Frees native-owned commit-list arrays.
    pub fn arcadia_tio_commit_list_free(commits: *mut ArcadiaTioCommitList);
}
