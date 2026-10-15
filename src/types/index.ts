export interface SoulConfig { rounds: number; max_minutes: number; stage_timeout_seconds: number; layer: number }
export interface Config { schema_version: number; endpoint: string; serial: string; soul: SoulConfig }
export interface Limits { rounds: [number, number]; max_minutes: [number, number]; stage_timeout_seconds: [number, number]; layers: number[] }
export interface Device { serial: string; status: string; description: string }
export interface RunState {
  run_id: string; device_id: string; status: string; stage: string; completed_rounds: number;
  started_at_ms: number; ended_at_ms: number | null; message: string; failure_screenshot: string | null;
}
export interface Log { sequence: number; timestamp_ms: number; level: string; device_id: string; run_id: string | null; message: string }
export interface RepeatSchedule { interval_minutes: number; total_runs: number }
export interface QueueEntry { repeat: RepeatSchedule | null; completed_runs: number; last_run: RunState | null; id: string; config: Config; priority: number; due_at_ms: number; status: string; run_id: string | null; message: string }
export interface TaskQueue { enabled: boolean; entries: QueueEntry[] }
export interface Snapshot { queue: TaskQueue; sequence: number; config: Config; run: RunState | null; logs: Log[]; adb_source: string; data_directory: string }
export interface Bootstrap { snapshot: Snapshot; limits: Limits }
export interface Screenshot { data_url: string; width: number; height: number }
export interface VisualPreview {
  screenshot: Screenshot; page: string; page_label: string; reason: string;
  suggested: 'Wait' | 'Complete' | { Tap: { target: string; point: [number, number]; expected: string[] } };
  observation: { compute?: {provider:string;fallback_reason:string|null}; dimensions: [number, number]; detections: { class: string; confidence: number; bounds: [number, number, number, number] }[];
    timings: { decode_ms?: number; capture_mode?: string; capture_read_ms?: number; capture_encode_ms?: number; recognition_ms?: number; scene_ms?: number; template_ms?: number; click_ms?: number; screenshot_write_ms?: number; screenshot_ms: number; preprocess_ms: number; inference_ms: number; postprocess_ms: number; decision_ms: number; total_ms: number } };
}
export interface Match { id: string; score: number; found: boolean; bounds: [number, number, number, number] }
export interface Viewport { source_size: [number, number]; rotation: number; content_rect: [number, number, number, number]; working_size: [number, number]; strategy: 'stretch' | 'anchored' | 'letterbox' }
export interface Diagnosis { matches: Match[]; viewport: Viewport; alignment_confident: boolean; elapsed_ms: number; ocr: string; matcher: string }
export interface VerifiedClick { screenshot: Screenshot; diagnosis: Diagnosis; target_disappeared: boolean; verification: 'disappeared' | 'still_present' | 'unconfirmed' }

export interface ImportPreview { base: Config; config: Config; mappings: { source: string; target: string; before: unknown; after: unknown }[]; warnings: string[]; errors: string[]; ignored: string[] }
