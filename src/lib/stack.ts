import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, type ServiceInfo, type ServiceKind, type Settings } from "./api";
import { useAppStore } from "./store";

/** Services shown (and started by "Start all") with these settings. */
export function serverEnabled(kind: ServiceKind, mode: Settings["webServer"] | undefined) {
  if (kind === "apache") return mode !== "nginx";
  if (kind === "nginx") return mode === "nginx" || mode === "both";
  return true;
}

/**
 * Starting and stopping groups of services: the active stack profile, a selection, or
 * everything. Shared by the Overview header, the title bar and the Shift+S / Shift+X keys.
 */
export function useStack() {
  const queryClient = useQueryClient();
  const notify = useAppStore((s) => s.notify);
  const activeName = useAppStore((s) => s.activeProfile);
  const services = useQuery({ queryKey: ["services"], queryFn: api.listServices });
  const settings = useQuery({ queryKey: ["settings"], queryFn: api.getSettings });
  const profile = settings.data?.profiles.find((p) => p.name === activeName) ?? null;

  const usable = (services.data ?? []).filter((s) => s.installed && serverEnabled(s.kind, settings.data?.webServer));
  const running = usable.filter((s) => s.state === "running" || s.state === "starting");

  const apply = useMutation({
    mutationFn: ({ start, stop }: { start: ServiceKind[]; stop: ServiceKind[] }) => api.applyServices(start, stop),
    // The Overview shows each failure on its service's row; elsewhere a toast says it.
    onSuccess: (errors) => useAppStore.getState().page !== "overview" && errors.forEach((e) => notify("error", e)),
    onError: (e) => notify("error", String(e)),
    onSettled: () => queryClient.invalidateQueries({ queryKey: ["services"] }),
  });

  const kinds = (list: ServiceInfo[]) => list.map((s) => s.kind);
  return {
    profile,
    usable,
    running,
    busy: apply.isPending,
    start: (list: ServiceKind[]) => apply.mutate({ start: list, stop: [] }),
    stop: (list: ServiceKind[]) => apply.mutate({ start: [], stop: list }),
    /** The active profile's services, stopping running ones outside it; otherwise everything. */
    startStack: () =>
      profile
        ? apply.mutate({ start: profile.services, stop: kinds(running).filter((k) => !profile.services.includes(k)) })
        : apply.mutate({ start: kinds(usable), stop: [] }),
    startProfile: (services: ServiceKind[]) =>
      apply.mutate({ start: services, stop: kinds(running).filter((k) => !services.includes(k)) }),
    stopAll: () => apply.mutate({ start: [], stop: kinds(running) }),
  };
}
