interface EmptyStateProps {
  title: string;
  description: string;
  actionLabel?: string;
  onAction?: () => void;
}

export default function EmptyState({ title, description, actionLabel, onAction }: EmptyStateProps) {
  return (
    <div className="emptyState card">
      <strong>{title}</strong>
      <p>{description}</p>
      {actionLabel && onAction && (
        <button className="primaryButton" onClick={onAction} type="button">{actionLabel}</button>
      )}
    </div>
  );
}
