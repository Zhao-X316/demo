-- M1.2：总体终审效果与共享 learning_evidence 的数据库级回滚顺序保护。
--
-- 业务事务必须先把对应的 active 背诵证据切换为 reverted，再回滚逐点评审，
-- 最后才能把 decision_effects 切换为 reverted。这样即使服务层未来误排顺序，
-- 数据库也不会留下“排程已回滚、正式学习证据仍生效”的半套状态。

CREATE TRIGGER trg_rec_learning_evidence_before_effect_revert
BEFORE UPDATE OF state ON decision_effects
WHEN OLD.module='recitation'
  AND OLD.state='active'
  AND NEW.state='reverted'
  AND EXISTS (
    SELECT 1
    FROM learning_evidence evidence
    WHERE evidence.source_module='recitation'
      AND evidence.decision_ref_type='recitation_decision_effect'
      AND evidence.decision_ref_id=CAST(OLD.id AS TEXT)
      AND evidence.decision_revision=OLD.revision
      AND evidence.state='active'
  )
BEGIN
  SELECT RAISE(ABORT,'M1_LEARNING_EVIDENCE_MUST_REVERT_FIRST');
END;
