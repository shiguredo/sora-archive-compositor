//! コールバック型コーデックの結果キュー（fail-fast）。
//!
//! VideoToolbox / NVCODEC のように非同期コールバックで結果が届く経路向け。
//! 最初のエラーで終端し、以降の成功フレームは捨て、後続エラーは無視する。
//! 跨スレッド利用時は呼び出し側で `Mutex` 等により排他し、`T: Send` を満たすこと。

use std::collections::VecDeque;

/// コールバック結果を fail-fast 契約で保持するキュー。
///
/// - 成功は終端前だけ蓄積する
/// - 最初のエラーで終端し、既に積んだ成功は先に取り出せる（到着順）
/// - 終端後の `push_ok` は捨て、後続の `push_err` は無視する
/// - 未配信の終端エラーがあるとき、成功を出し切ったあとの `pop` はちょうど 1 回 `Err` を返す
/// - 相手が既にエラーを `pop` 済みのキューを `append_from` した場合は、
///   エラー無しで終端し、以降の `pop` は `Ok(None)` のみになる
#[derive(Debug)]
pub struct OutputQueue<T> {
    ok: VecDeque<T>,
    /// まだ `pop` していない終端エラー
    pending_error: Option<crate::Error>,
    /// 終端済み（自前の `pop` でエラー配信済み、または `append_from` で継承した終端）
    error_delivered: bool,
}

impl<T> Default for OutputQueue<T> {
    fn default() -> Self {
        Self {
            ok: VecDeque::new(),
            pending_error: None,
            error_delivered: false,
        }
    }
}

impl<T> OutputQueue<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// 終端済みか（未配信エラーあり、または終端フラグ済み）。
    fn is_terminated(&self) -> bool {
        self.pending_error.is_some() || self.error_delivered
    }

    /// 成功結果を積む。終端後は捨てる。
    pub fn push_ok(&mut self, item: T) {
        if self.is_terminated() {
            return;
        }
        self.ok.push_back(item);
    }

    /// エラーで終端する。既に終端していれば無視する。
    pub fn push_err(&mut self, err: crate::Error) {
        if self.is_terminated() {
            return;
        }
        self.pending_error = Some(err);
    }

    /// 成功を 1 件取り出す。尽きていれば未配信エラーを 1 回だけ `Err` で返す。
    pub fn pop(&mut self) -> crate::Result<Option<T>> {
        if let Some(item) = self.ok.pop_front() {
            return Ok(Some(item));
        }
        if let Some(err) = self.pending_error.take() {
            self.error_delivered = true;
            return Err(err);
        }
        Ok(None)
    }

    /// 別キューの未消費分を取り込む（VideoToolbox デコーダ再初期化用）。
    ///
    /// 自身が既に終端していれば相手の内容は捨てる。
    /// 相手に未配信の終端エラーがあれば、自身の成功の後ろにそのエラーを引き継ぐ。
    /// 相手がエラー配信済みだけで未配信エラーが無い場合は、自身もエラー無しで終端する。
    pub fn append_from(&mut self, other: &mut Self) {
        if self.is_terminated() {
            other.ok.clear();
            other.pending_error = None;
            other.error_delivered = true;
            return;
        }
        self.ok.append(&mut other.ok);
        if let Some(err) = other.pending_error.take() {
            self.pending_error = Some(err);
        }
        // 相手がエラー配信済みだけの場合も終端として扱う
        if other.error_delivered && self.pending_error.is_none() {
            self.error_delivered = true;
        }
        other.error_delivered = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pops_ok_items_in_order() {
        let mut q = OutputQueue::new();
        q.push_ok(1);
        q.push_ok(2);
        assert_eq!(q.pop().expect("空でない"), Some(1));
        assert_eq!(q.pop().expect("空でない"), Some(2));
        assert_eq!(q.pop().expect("空"), None);
    }

    #[test]
    fn delivers_pending_oks_before_error() {
        let mut q = OutputQueue::new();
        q.push_ok(1);
        q.push_err(crate::Error::new("boom"));
        q.push_ok(2); // 終端後は捨てる
        assert_eq!(q.pop().expect("ok"), Some(1));
        let err = q.pop().expect_err("エラーを返す");
        assert!(err.display().contains("boom"));
        assert_eq!(q.pop().expect("配信後は空"), None);
    }

    #[test]
    fn ignores_subsequent_errors() {
        let mut q: OutputQueue<i32> = OutputQueue::new();
        q.push_err(crate::Error::new("first"));
        q.push_err(crate::Error::new("second"));
        let err = q.pop().expect_err("最初のエラー");
        assert!(err.display().contains("first"));
        assert!(!err.display().contains("second"));
    }

    #[test]
    fn append_from_merges_oks_then_other_error() {
        let mut a = OutputQueue::new();
        a.push_ok(1);
        let mut b = OutputQueue::new();
        b.push_ok(2);
        b.push_err(crate::Error::new("from-b"));
        a.append_from(&mut b);
        assert_eq!(a.pop().expect("ok"), Some(1));
        assert_eq!(a.pop().expect("ok"), Some(2));
        let err = a.pop().expect_err("b のエラー");
        assert!(err.display().contains("from-b"));
    }

    #[test]
    fn append_from_discards_other_when_self_terminated() {
        let mut a = OutputQueue::new();
        a.push_err(crate::Error::new("a-err"));
        let mut b = OutputQueue::new();
        b.push_ok(9);
        a.append_from(&mut b);
        let err = a.pop().expect_err("自身のエラー");
        assert!(err.display().contains("a-err"));
        assert_eq!(a.pop().expect("空"), None);
    }

    /// 相手が既にエラーを pop 済みのとき、受け側は Err 無しで終端し Ok(None) のみになる
    #[test]
    fn append_from_inherits_delivered_termination_without_pending_error() {
        let mut other = OutputQueue::new();
        other.push_err(crate::Error::new("already-popped"));
        let err = other.pop().expect_err("先にエラーを取り出す");
        assert!(err.display().contains("already-popped"));

        let mut self_q = OutputQueue::new();
        self_q.push_ok(1);
        self_q.append_from(&mut other);

        assert_eq!(self_q.pop().expect("引き継いだ ok"), Some(1));
        // 未配信エラーは無いので Err にはならず、終端後は空
        assert_eq!(self_q.pop().expect("エラー無し終端"), None);
        assert!(self_q.is_terminated());
        self_q.push_ok(2); // 終端後は捨てる
        assert_eq!(self_q.pop().expect("追加も空"), None);
    }
}
