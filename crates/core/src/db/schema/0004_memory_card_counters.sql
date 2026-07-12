-- 修正旧版把“未通过”也计入 reps、却从未增加 lapses 的脱档卡片。
-- state=lapsed 能证明最近一次结果为未通过；只搬移最近这一笔，不猜测更早历史。
-- 已有 decision_effects 的 scope 必须保持卡片与快照一致，跳过自动修复，留待显式迁移。
UPDATE memory_cards
SET reps = CASE WHEN reps > 0 THEN reps - 1 ELSE 0 END,
    lapses = lapses + 1,
    updated_at = datetime('now')
WHERE state = 'lapsed'
  AND lapses = 0
  AND NOT EXISTS (
      SELECT 1
      FROM decision_effects AS effect
      WHERE effect.module = memory_cards.module
        AND effect.student_id = memory_cards.student_id
        AND effect.ref_type = memory_cards.ref_type
        AND effect.ref_id = memory_cards.ref_id
  );
