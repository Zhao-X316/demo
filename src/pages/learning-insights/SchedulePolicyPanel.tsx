import { SchedulePolicy, updateWrongbookSchedulePolicy } from "../../api/learning";
import { useState, useEffect } from "react";

export function SchedulePolicyPanel({
  policy,
  onSaved,
}: {
  policy: SchedulePolicy;
  onSaved: (policy: SchedulePolicy) => void;
}) {
  const [delay, setDelay] = useState(policy.default_delay_days);
  const [dailyLimit, setDailyLimit] = useState(policy.daily_limit_per_student);
  const [skipWeekend, setSkipWeekend] = useState(policy.weekend_policy === "next_workday");
  const [skipHoliday, setSkipHoliday] = useState(policy.holiday_policy === "next_workday");
  const [holidays, setHolidays] = useState(policy.holidays);
  const [newHolidayDate, setNewHolidayDate] = useState("");
  const [newHolidayLabel, setNewHolidayLabel] = useState("");
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState("");

  useEffect(() => {
    setDelay(policy.default_delay_days);
    setDailyLimit(policy.daily_limit_per_student);
    setSkipWeekend(policy.weekend_policy === "next_workday");
    setSkipHoliday(policy.holiday_policy === "next_workday");
    setHolidays(policy.holidays);
  }, [policy]);

  const addHoliday = () => {
    if (!newHolidayDate || !newHolidayLabel.trim()) return;
    setHolidays((current) => [
      ...current.filter((item) => item.calendar_date !== newHolidayDate),
      { calendar_date: newHolidayDate, label: newHolidayLabel.trim() },
    ].sort((left, right) => left.calendar_date.localeCompare(right.calendar_date)));
    setNewHolidayDate("");
    setNewHolidayLabel("");
  };
  const save = async () => {
    setSaving(true);
    setSaveError("");
    try {
      const saved = await updateWrongbookSchedulePolicy({
        defaultDelayDays: delay,
        dailyLimitPerStudent: dailyLimit,
        weekendPolicy: skipWeekend ? "next_workday" : "allow",
        holidayPolicy: skipHoliday ? "next_workday" : "allow",
        maxShiftDays: policy.max_shift_days,
        holidays,
      });
      onSaved(saved);
    } catch (reason) {
      setSaveError(String(reason));
    } finally {
      setSaving(false);
    }
  };

  return (
    <section className="schedule-policy-panel">
      <div className="schedule-policy-title">
        <div>
          <b>巩固规则</b>
          <span>默认自动计算日期，但只有老师确认后才会建立任务。</span>
        </div>
        <span>当前第 {policy.revision} 版</span>
      </div>
      <div className="schedule-policy-main">
        <label>
          <span>订正后间隔</span>
          <div><input type="number" min={1} max={60} value={delay}
            onChange={(event) => setDelay(Number(event.target.value))} /> 天</div>
        </label>
        <label>
          <span>每名学生每天最多</span>
          <div><input type="number" min={1} max={20} value={dailyLimit}
            onChange={(event) => setDailyLimit(Number(event.target.value))} /> 项</div>
        </label>
        <label className="schedule-policy-check">
          <input type="checkbox" checked={skipWeekend}
            onChange={(event) => setSkipWeekend(event.target.checked)} />
          周末顺延
        </label>
        <label className="schedule-policy-check">
          <input type="checkbox" checked={skipHoliday}
            onChange={(event) => setSkipHoliday(event.target.checked)} />
          登记假期顺延
        </label>
      </div>
      <details className="schedule-holidays">
        <summary>校历假期（{holidays.length} 天）</summary>
        <div className="schedule-holiday-add">
          <input aria-label="假期日期" type="date" value={newHolidayDate}
            onChange={(event) => setNewHolidayDate(event.target.value)} />
          <input aria-label="假期名称" value={newHolidayLabel} maxLength={40}
            placeholder="如：校运动会" onChange={(event) => setNewHolidayLabel(event.target.value)} />
          <button disabled={!newHolidayDate || !newHolidayLabel.trim()} onClick={addHoliday}>
            添加
          </button>
        </div>
        <div className="schedule-holiday-list">
          {holidays.map((holiday) => (
            <span key={holiday.calendar_date}>
              {holiday.calendar_date} · {holiday.label}
              <button aria-label={`删除 ${holiday.label}`}
                onClick={() => setHolidays((current) =>
                  current.filter((item) => item.calendar_date !== holiday.calendar_date))}>×</button>
            </span>
          ))}
          {holidays.length === 0 && <em>暂未登记假期</em>}
        </div>
      </details>
      {saveError && <div className="error">{saveError}</div>}
      <div className="schedule-policy-actions">
        <span>修改后生成新版本，已经安排的任务日期不会被静默改动。</span>
        <button className="primary" disabled={saving} onClick={save}>
          {saving ? "保存中…" : "保存规则"}
        </button>
      </div>
    </section>
  );
}
