-- 班级绑定教材（学科+年级册，如「道法8上」），用于布置时默认到该班教材。
ALTER TABLE classes ADD COLUMN textbook TEXT;
