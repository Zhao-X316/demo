import type { ClassProfileScopeInput } from "./profiles";
import { call } from "../client";

export interface ClassTeachingEvent {
  public_id: string;
  event_key: string;
  class_id: number;
  revision: number;
  event_type: "new_lesson" | "review" | "quiz" | "exam" | "holiday" | "schedule_pause";
  title: string;
  range_start: string;
  range_end: string;
  note: string | null;
  state: "active" | "voided";
  supersedes_public_id: string | null;
  created_by: string;
  created_at: string;
}

export interface CreateClassTeachingEventInput {
  requestKey: string;
  classId: number;
  eventType: ClassTeachingEvent["event_type"];
  title: string;
  rangeStart: string;
  rangeEnd: string;
  note?: string | null;
}

export interface ReviseClassTeachingEventInput {
  requestKey: string;
  eventKey: string;
  expectedRevision: number;
  eventType: ClassTeachingEvent["event_type"];
  title: string;
  rangeStart: string;
  rangeEnd: string;
  note?: string | null;
}

export interface VoidClassTeachingEventInput {
  requestKey: string;
  eventKey: string;
  expectedRevision: number;
}

export const listClassTeachingEvents = (input: ClassProfileScopeInput) =>
  call<ClassTeachingEvent[]>("list_class_teaching_events", { input });

export const createClassTeachingEvent = (input: CreateClassTeachingEventInput) =>
  call<ClassTeachingEvent>("create_class_teaching_event", { input });

export const reviseClassTeachingEvent = (input: ReviseClassTeachingEventInput) =>
  call<ClassTeachingEvent>("revise_class_teaching_event", { input });

export const voidClassTeachingEvent = (input: VoidClassTeachingEventInput) =>
  call<ClassTeachingEvent>("void_class_teaching_event", { input });
