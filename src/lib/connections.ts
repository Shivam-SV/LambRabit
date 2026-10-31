import type { ServiceKind } from "./api";

/** How an app connects to a service: a URL, and the same as .env lines. */
export interface Connection {
  url: string;
  env: [string, string][];
}

// 127.0.0.1 rather than localhost: MySQL clients (PHP's included) read "localhost" as
// "use the Unix socket", which isn't where LampRabit's servers listen.
const HOST = "127.0.0.1";

export function connection(kind: ServiceKind, port: number): Connection {
  switch (kind) {
    case "apache":
    case "nginx": {
      const url = `http://localhost${port === 80 ? "" : `:${port}`}`;
      return { url, env: [["APP_URL", url]] };
    }
    case "mariadb":
    case "mysql":
      return {
        url: `${kind}://root@${HOST}:${port}`,
        env: [
          ["DB_CONNECTION", kind],
          ["DB_HOST", HOST],
          ["DB_PORT", String(port)],
          ["DB_USERNAME", "root"],
          ["DB_PASSWORD", ""],
        ],
      };
    case "redis":
      return {
        url: `redis://${HOST}:${port}`,
        env: [
          ["REDIS_HOST", HOST],
          ["REDIS_PASSWORD", "null"],
          ["REDIS_PORT", String(port)],
        ],
      };
    case "memcached":
      return {
        url: `${HOST}:${port}`,
        env: [
          ["MEMCACHED_HOST", HOST],
          ["MEMCACHED_PORT", String(port)],
        ],
      };
    case "mailpit":
      return {
        url: `smtp://${HOST}:${port}`,
        env: [
          ["MAIL_MAILER", "smtp"],
          ["MAIL_HOST", HOST],
          ["MAIL_PORT", String(port)],
          ["MAIL_USERNAME", "null"],
          ["MAIL_PASSWORD", "null"],
          ["MAIL_ENCRYPTION", "null"],
        ],
      };
  }
}

export const envText = (env: [string, string][]) => env.map(([k, v]) => `${k}=${v}`).join("\n");

/** The text copied for a connection in the given mode. */
export const copyText = (c: Connection, mode: "url" | "env") => (mode === "url" ? c.url : envText(c.env));
