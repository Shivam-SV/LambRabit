import type { LogService, LogStream, RawLogLine } from "./api";

export type Level = "error" | "warn" | "notice" | "info" | "access" | "debug";

export interface LogEntry {
  id: number;
  service: LogService;
  stream: LogStream;
  /** For service "project". */
  project?: string;
  time: Date;
  /** False when the line carried no timestamp and `time` is when it was read. */
  timed: boolean;
  level: Level;
  message: string;
}

const apacheLevels: Record<string, Level> = {
  emerg: "error",
  alert: "error",
  crit: "error",
  error: "error",
  warn: "warn",
  notice: "notice",
  info: "info",
};

function validDate(d: Date | null): Date | null {
  return d && !Number.isNaN(d.getTime()) ? d : null;
}

function levelFromText(text: string): Level {
  // Summaries like "0 warnings and 0 errors" or "0 failed" are good news.
  text = text.replace(/(no|0) (errors?|warnings?|failed|failures?)/gi, "");
  if (/\b(fatal|error|failed|crit)/i.test(text)) return "error";
  if (/\bwarn/i.test(text)) return "warn";
  if (/\b(notice|deprecated)/i.test(text)) return "notice";
  return "info";
}

/** Extracts time, level and message from the formats Apache, MariaDB and PHP write. */
export function parseLog(raw: RawLogLine, received = new Date()): LogEntry {
  const base = { id: raw.id, service: raw.service, stream: raw.stream, project: raw.project, timed: true };
  const line = raw.line;
  let m: RegExpMatchArray | null;

  // A project's output: [2026-10-31 12:00:00] output|error|command: text
  if ((m = line.match(/^\[(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2}:\d{2})\] (output|error|command): (.*)$/))) {
    const message = m[4];
    return {
      ...base,
      stream: m[3] as LogStream,
      time: validDate(new Date(`${m[1]}T${m[2]}`)) ?? received,
      level: m[3] === "command" ? "notice" : levelFromText(message),
      message,
    };
  }

  // LampRabit system log: [2026-10-24 18:11:14] info: Apache started
  if ((m = line.match(/^\[(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2}:\d{2})\] (info|warn|error): (.*)$/))) {
    return {
      ...base,
      time: validDate(new Date(`${m[1]}T${m[2]}`)) ?? received,
      level: m[3] as Level,
      message: m[4],
    };
  }

  // Apache access: 127.0.0.1 - - [24/Oct/2026:17:56:01 +0530] "GET / HTTP/1.1" 200 1234
  if (raw.stream === "access" && (m = line.match(/\[(\d{2})\/(\w{3})\/(\d{4}):(\d{2}:\d{2}:\d{2}) ([+-]\d{4})\]/))) {
    const status = Number(line.match(/" (\d{3}) /)?.[1]);
    return {
      ...base,
      time: validDate(new Date(`${m[1]} ${m[2]} ${m[3]} ${m[4]} ${m[5]}`)) ?? received,
      level: status >= 500 ? "error" : status >= 400 ? "warn" : "access",
      message: line,
    };
  }

  // Apache error: [Sat Oct 24 17:55:59.123456 2026] [core:warn] [pid 1:tid 2] AH00098: ...
  if ((m = line.match(/^\[\w{3} (\w{3}) +(\d{1,2}) (\d{2}:\d{2}:\d{2})(?:\.\d+)? (\d{4})\] \[[\w-]*:(\w+)\](?: \[pid [^\]]*\])?(?: \[client [^\]]*\])? ?(.*)$/))) {
    return {
      ...base,
      time: validDate(new Date(`${m[1]} ${m[2]} ${m[4]} ${m[3]}`)) ?? received,
      level: apacheLevels[m[5]] ?? (m[5].startsWith("trace") || m[5] === "debug" ? "debug" : "info"),
      message: m[6],
    };
  }

  // Nginx: 2026/10/24 19:00:00 [error] 1234#5678: *1 message
  if ((m = line.match(/^(\d{4})\/(\d{2})\/(\d{2}) (\d{2}:\d{2}:\d{2}) \[(\w+)\] \d+#\d+: (?:\*\d+ )?(.*)$/))) {
    const kind = m[5];
    return {
      ...base,
      time: validDate(new Date(`${m[1]}-${m[2]}-${m[3]}T${m[4]}`)) ?? received,
      level: ["emerg", "alert", "crit", "error"].includes(kind) ? "error" : kind === "warn" ? "warn" : kind === "notice" ? "notice" : "info",
      message: m[6],
    };
  }

  // MariaDB: 2026-10-24 17:43:11 0 [Note] InnoDB: ...
  if ((m = line.match(/^(\d{4}-\d{2}-\d{2}) +(\d{1,2}):(\d{2}:\d{2}) +\d+ \[(\w+)\] (.*)$/))) {
    const kind = m[4].toLowerCase();
    return {
      ...base,
      time: validDate(new Date(`${m[1]}T${m[2].padStart(2, "0")}:${m[3]}`)) ?? received,
      level: kind === "error" ? "error" : kind === "warning" ? "warn" : "info",
      message: `[${m[4]}] ${m[5]}`,
    };
  }

  // PHP: [24-Oct-2026 12:00:00 UTC] PHP Warning:  ...   /   php-fpm: [24-Oct-2026 12:00:00] NOTICE: ...
  if ((m = line.match(/^\[(\d{2})-(\w{3})-(\d{4}) (\d{2}:\d{2}:\d{2})(?: ([\w/+-]+))?\] (.*)$/))) {
    const zone = m[5] === "UTC" ? " GMT" : "";
    const message = m[6].replace(/^PHP +/, "");
    return {
      ...base,
      time: validDate(new Date(`${m[1]} ${m[2]} ${m[3]} ${m[4]}${zone}`)) ?? received,
      level: levelFromText(message.split(":")[0]),
      message,
    };
  }

  return { ...base, timed: false, time: received, level: levelFromText(line), message: line };
}
