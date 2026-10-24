import { Card, PageHeader } from "../components/ui";

export default function Placeholder({ title, phase, text }: { title: string; phase: number; text: string }) {
  return (
    <>
      <PageHeader title={title} />
      <div className="px-8">
        <Card className="p-6 text-sm text-zinc-500">
          <span className="mr-2 rounded bg-zinc-100 px-1.5 py-0.5 text-xs font-medium dark:bg-zinc-800">
            Phase {phase}
          </span>
          {text}
        </Card>
      </div>
    </>
  );
}
