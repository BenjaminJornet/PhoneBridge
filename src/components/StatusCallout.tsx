import type { StatusTone } from "../lib/ux";

interface StatusCalloutProps {
  title?: string;
  message: string;
  tone?: StatusTone;
}

export default function StatusCallout({ title = "Status", message, tone = "info" }: StatusCalloutProps) {
  return (
    <div className={`statusCallout ${tone}`}>
      <strong>{title}</strong>
      <span>{message}</span>
    </div>
  );
}
