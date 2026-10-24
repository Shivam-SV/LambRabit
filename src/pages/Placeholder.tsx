import { Card, PageHeader } from "../components/ui";

export default function Placeholder({ title, phase, text }: { title: string; phase: number; text: string }) {
  return (
    <>
      <PageHeader title={title} />
      <div className="px-8">
        <Card className="p-6 text-sm text-muted">
          <span className="mr-2 rounded bg-accent-soft px-1.5 py-0.5 text-xs font-medium text-accent-text">
            Phase {phase}
          </span>
          {text}
        </Card>
      </div>
    </>
  );
}
