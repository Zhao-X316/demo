import {
  ClassTeachingEvent,
  ClassProfileSnapshot,
  ClassOperationsDashboard,
  ClassProfilePreview,
  loadClassOperationsDashboard,
  loadLatestClassProfile,
  listClassTeachingEvents,
  previewClassProfile,
  generateClassProfile,
  createClassProfileExportSnapshot,
  writeClassProfileExportSnapshot,
  reviseClassTeachingEvent,
  createClassTeachingEvent,
  voidClassTeachingEvent,
} from "../../api/classDashboard";
import { useState, useMemo, useEffect } from "react";
import { Class, classesList } from "../../api/manage";
import { ProfileScopeOption, loadProfileScopeOptions } from "../../api/learning";
import { save } from "@tauri-apps/plugin-dialog";
import { newRequestKey } from "./shared";

function localDate() {
  const now = new Date();
  const year = now.getFullYear();
  const month = String(now.getMonth() + 1).padStart(2, "0");
  const day = String(now.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function daysBefore(value: string, days: number) {
  const date = new Date(`${value}T12:00:00`);
  date.setDate(date.getDate() - days);
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

interface TeachingEventDraft {
  eventType: ClassTeachingEvent["event_type"];
  title: string;
  rangeStart: string;
  rangeEnd: string;
  note: string;
}

function classProfileExportFileName(snapshot: ClassProfileSnapshot) {
  const safeClassName = snapshot.class.name.replace(/[<>:"/\\|?*\u0000-\u001f]/g, "_").trim();
  return `${safeClassName || "班级"}_班级掌握脱敏摘要_${snapshot.range_start}_至${snapshot.range_end}.csv`;
}

export function useClassDashboardController(initialClassId?: number) {
  const [classes, setClasses] = useState<Class[]>([]);
  const [classId, setClassId] = useState<number | null>(initialClassId || null);
  useEffect(()=>{if(initialClassId)setClassId(initialClassId);},[initialClassId]);
  const [asOfDate, setAsOfDate] = useState(localDate);
  const [dashboard, setDashboard] = useState<ClassOperationsDashboard | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [refreshTick, setRefreshTick] = useState(0);
  const [profileRangeStart, setProfileRangeStart] = useState(() => daysBefore(localDate(), 29));
  const [profileRangeEnd, setProfileRangeEnd] = useState(localDate);
  const [profileScopeOptions, setProfileScopeOptions] = useState<ProfileScopeOption[]>([]);
  const [profileScopeSelectorKey, setProfileScopeSelectorKey] =
    useState("auto_evidence_maps");
  const [profilePreview, setProfilePreview] = useState<ClassProfilePreview | null>(null);
  const [profileSnapshot, setProfileSnapshot] = useState<ClassProfileSnapshot | null>(null);
  const [profileLoading, setProfileLoading] = useState(false);
  const [profileError, setProfileError] = useState("");
  const [profileExporting, setProfileExporting] = useState(false);
  const [profileExportError, setProfileExportError] = useState("");
  const [profileExportedFile, setProfileExportedFile] = useState("");
  const [profileView, setProfileView] = useState<"knowledge" | "ability">("knowledge");
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [showTeachingInputBuilder, setShowTeachingInputBuilder] = useState(false);
  const [teachingEvents, setTeachingEvents] = useState<ClassTeachingEvent[]>([]);
  const [teachingEventLoading, setTeachingEventLoading] = useState(false);
  const [teachingEventError, setTeachingEventError] = useState("");
  const [editingTeachingEvent, setEditingTeachingEvent] = useState<ClassTeachingEvent | null>(null);
  const [showTeachingEventForm, setShowTeachingEventForm] = useState(false);
  const [teachingEventDraft, setTeachingEventDraft] = useState<TeachingEventDraft>(() => ({
    eventType: "new_lesson",
    title: "",
    rangeStart: localDate(),
    rangeEnd: localDate(),
    note: "",
  }));
  const selectedProfileScope = useMemo(
    () => profileScopeOptions.find(
      (item) => item.selector_key === profileScopeSelectorKey,
    ) ?? profileScopeOptions[0] ?? null,
    [profileScopeOptions, profileScopeSelectorKey],
  );

  useEffect(() => {
    let current = true;
    loadProfileScopeOptions()
      .then((items) => {
        if (!current) return;
        setProfileScopeOptions(items);
        setProfileScopeSelectorKey((selected) =>
          items.some((item) => item.selector_key === selected)
            ? selected
            : items[0]?.selector_key ?? "auto_evidence_maps");
      })
      .catch((reason) => {
        if (current) setProfileError(String(reason));
      });
    return () => {
      current = false;
    };
  }, []);

  useEffect(() => {
    classesList()
      .then((items) => {
        setClasses(items);
        setClassId((current) => current ?? items[0]?.id ?? null);
        if (items.length === 0) setLoading(false);
      })
      .catch((reason) => {
        setError(String(reason));
        setLoading(false);
      });
  }, []);

  useEffect(() => {
    if (classId === null) {
      setDashboard(null);
      return;
    }
    let current = true;
    setLoading(true);
    setError("");
    loadClassOperationsDashboard(classId, asOfDate)
      .then((value) => {
        if (current) setDashboard(value);
      })
      .catch((reason) => {
        if (current) {
          setDashboard(null);
          setError(String(reason));
        }
      })
      .finally(() => {
        if (current) setLoading(false);
      });
    return () => {
      current = false;
    };
  }, [classId, asOfDate, refreshTick]);

  useEffect(() => {
    if (classId === null) {
      setProfileSnapshot(null);
      setProfilePreview(null);
      return;
    }
    let current = true;
    setProfileLoading(true);
    setProfileError("");
    setProfilePreview(null);
    setSelectedNodeId(null);
    setShowTeachingInputBuilder(false);
    setProfileExportError("");
    setProfileExportedFile("");
    loadLatestClassProfile(classId)
      .then((value) => {
        if (current) setProfileSnapshot(Array.isArray(value) ? null : value);
      })
      .catch((reason) => {
        if (current) {
          setProfileSnapshot(null);
          setProfileError(String(reason));
        }
      })
      .finally(() => {
        if (current) setProfileLoading(false);
      });
    return () => {
      current = false;
    };
  }, [classId]);

  useEffect(() => {
    if (classId === null) {
      setTeachingEvents([]);
      return;
    }
    let current = true;
    setTeachingEventLoading(true);
    setTeachingEventError("");
    listClassTeachingEvents({
      classId,
      rangeStart: profileRangeStart,
      rangeEnd: profileRangeEnd,
    })
      .then((items) => {
        if (current) setTeachingEvents(items);
      })
      .catch((reason) => {
        if (current) {
          setTeachingEvents([]);
          setTeachingEventError(String(reason));
        }
      })
      .finally(() => {
        if (current) setTeachingEventLoading(false);
      });
    return () => {
      current = false;
    };
  }, [classId, profileRangeStart, profileRangeEnd]);

  const exceptionCount = useMemo(
    () =>
      (dashboard?.recitation.recognition_failure_count ?? 0)
      + (dashboard?.exam.open_pipeline_issue_count ?? 0),
    [dashboard],
  );

  const profileMetrics = profileView === "knowledge"
    ? profileSnapshot?.knowledge_metrics ?? []
    : profileSnapshot?.ability_metrics ?? [];
  const selectedNode = profileMetrics.find((item) => item.public_id === selectedNodeId) ?? null;
  const commonSupportNodes = profileSnapshot?.knowledge_metrics.filter(
    (item) => item.class_status === "common_needs_support",
  ) ?? [];
  const commonTeachingInputCount = profileSnapshot
    ? [...profileSnapshot.knowledge_metrics, ...profileSnapshot.ability_metrics]
      .filter((item) =>
        item.class_status === "common_needs_support" && item.sample_sufficient,
      ).length
    : 0;

  const runProfilePreview = async () => {
    if (classId === null) return;
    setProfileLoading(true);
    setProfileError("");
    try {
      const value = await previewClassProfile({
        classId,
        rangeStart: profileRangeStart,
        rangeEnd: profileRangeEnd,
        scopeSelectorKind: selectedProfileScope?.selector_kind ?? "auto_evidence_maps",
        scopeSelectorPublicId: selectedProfileScope?.selector_public_id ?? null,
      });
      setProfilePreview(value);
    } catch (reason) {
      setProfilePreview(null);
      setProfileError(String(reason));
    } finally {
      setProfileLoading(false);
    }
  };

  const confirmProfileGeneration = async () => {
    if (classId === null || !profilePreview) return;
    setProfileLoading(true);
    setProfileError("");
    try {
      const value = await generateClassProfile({
        classId,
        rangeStart: profileRangeStart,
        rangeEnd: profileRangeEnd,
        scopeSelectorKind: selectedProfileScope?.selector_kind ?? "auto_evidence_maps",
        scopeSelectorPublicId: selectedProfileScope?.selector_public_id ?? null,
        expectedSourceWatermark: profilePreview.source_watermark,
      });
      setProfileSnapshot(value);
      setProfilePreview(null);
      setSelectedNodeId(null);
      setShowTeachingInputBuilder(false);
      setProfileExportError("");
      setProfileExportedFile("");
    } catch (reason) {
      setProfileError(String(reason));
    } finally {
      setProfileLoading(false);
    }
  };

  const exportClassProfile = async () => {
    if (!profileSnapshot || profileSnapshot.is_stale) return;
    setProfileExporting(true);
    setProfileExportError("");
    setProfileExportedFile("");
    try {
      const outputPath = await save({
        defaultPath: classProfileExportFileName(profileSnapshot),
        filters: [{ name: "CSV 表格", extensions: ["csv"] }],
      });
      if (!outputPath) return;
      const exportSnapshot = await createClassProfileExportSnapshot(
        newRequestKey("class-profile-export"),
        profileSnapshot.public_id,
        profileSnapshot.payload_sha256,
      );
      const written = await writeClassProfileExportSnapshot(
        exportSnapshot.public_id,
        outputPath,
      );
      setProfileExportedFile(written.file_name);
    } catch (reason) {
      setProfileExportError(String(reason));
    } finally {
      setProfileExporting(false);
    }
  };

  const startTeachingEvent = (event?: ClassTeachingEvent) => {
    setEditingTeachingEvent(event ?? null);
    setTeachingEventDraft(event ? {
      eventType: event.event_type,
      title: event.title,
      rangeStart: event.range_start,
      rangeEnd: event.range_end,
      note: event.note ?? "",
    } : {
      eventType: "new_lesson",
      title: "",
      rangeStart: profileRangeEnd,
      rangeEnd: profileRangeEnd,
      note: "",
    });
    setTeachingEventError("");
    setShowTeachingEventForm(true);
  };

  const reloadTeachingEvents = async () => {
    if (classId === null) return;
    const items = await listClassTeachingEvents({
      classId,
      rangeStart: profileRangeStart,
      rangeEnd: profileRangeEnd,
    });
    setTeachingEvents(items);
  };

  const saveTeachingEvent = async () => {
    if (classId === null || !teachingEventDraft.title.trim()) return;
    setTeachingEventLoading(true);
    setTeachingEventError("");
    try {
      const common = {
        eventType: teachingEventDraft.eventType,
        title: teachingEventDraft.title.trim(),
        rangeStart: teachingEventDraft.rangeStart,
        rangeEnd: teachingEventDraft.rangeEnd,
        note: teachingEventDraft.note.trim() || null,
      };
      if (editingTeachingEvent) {
        await reviseClassTeachingEvent({
          requestKey: newRequestKey("class-teaching-event-revise"),
          eventKey: editingTeachingEvent.event_key,
          expectedRevision: editingTeachingEvent.revision,
          ...common,
        });
      } else {
        await createClassTeachingEvent({
          requestKey: newRequestKey("class-teaching-event-create"),
          classId,
          ...common,
        });
      }
      await reloadTeachingEvents();
      setShowTeachingEventForm(false);
      setEditingTeachingEvent(null);
    } catch (reason) {
      setTeachingEventError(String(reason));
    } finally {
      setTeachingEventLoading(false);
    }
  };

  const voidTeachingEvent = async (event: ClassTeachingEvent) => {
    if (!window.confirm(`作废“${event.title}”？历史 revision 会保留。`)) return;
    setTeachingEventLoading(true);
    setTeachingEventError("");
    try {
      await voidClassTeachingEvent({
        requestKey: newRequestKey("class-teaching-event-void"),
        eventKey: event.event_key,
        expectedRevision: event.revision,
      });
      await reloadTeachingEvents();
    } catch (reason) {
      setTeachingEventError(String(reason));
    } finally {
      setTeachingEventLoading(false);
    }
  };
  return {
    classes,
    classId,
    setClassId,
    asOfDate,
    setAsOfDate,
    dashboard,
    loading,
    error,
    setRefreshTick,
    profileRangeStart,
    setProfileRangeStart,
    profileRangeEnd,
    setProfileRangeEnd,
    profileScopeOptions,
    profileScopeSelectorKey,
    setProfileScopeSelectorKey,
    profilePreview,
    setProfilePreview,
    profileSnapshot,
    profileLoading,
    profileError,
    profileExporting,
    profileExportError,
    profileExportedFile,
    profileView,
    setProfileView,
    setSelectedNodeId,
    showTeachingInputBuilder,
    setShowTeachingInputBuilder,
    teachingEvents,
    teachingEventLoading,
    teachingEventError,
    editingTeachingEvent,
    setEditingTeachingEvent,
    showTeachingEventForm,
    setShowTeachingEventForm,
    teachingEventDraft,
    setTeachingEventDraft,
    exceptionCount,
    profileMetrics,
    selectedNode,
    commonSupportNodes,
    commonTeachingInputCount,
    runProfilePreview,
    confirmProfileGeneration,
    exportClassProfile,
    startTeachingEvent,
    saveTeachingEvent,
    voidTeachingEvent,
  };
}
