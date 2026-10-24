import type { Runtime, SitesInfo } from "./api";

/**
 * Where a developer tool opens: its own domain once the hosts file knows it, otherwise
 * the localhost alias (which works without the hosts file). Mailpit is reached directly.
 */
export function toolUrl(runtime: Runtime, sites: SitesInfo | undefined, mailUi: number | undefined): string | undefined {
  if (runtime === "mailpit") return mailUi ? `http://localhost:${mailUi}/` : undefined;
  const tool = sites?.tools.find((t) => t.tool === runtime);
  if (!tool) return undefined;
  return sites?.hostsOk ? tool.url : `http://localhost/${runtime}/`;
}
