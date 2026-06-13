interface StatsCardProps {
  label: string;
  value: string;
  detail: string;
}

export default function StatsCard({ label, value, detail }: StatsCardProps) {
  return (
    <article className="card statCard">
      <span>{label}</span>
      <strong>{value}</strong>
      <small>{detail}</small>
    </article>
  );
}
