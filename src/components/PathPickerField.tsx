import { useState } from "react";
import { shortenPath } from "../lib/ux";

interface PathPickerFieldProps {
  label: string;
  value: string;
  description: string;
  buttonLabel: string;
  onChoose: () => void;
  onChange?: (value: string) => void;
}

export default function PathPickerField({
  label,
  value,
  description,
  buttonLabel,
  onChoose,
  onChange,
}: PathPickerFieldProps) {
  const [showAdvanced, setShowAdvanced] = useState(false);

  return (
    <div className="pathPickerField">
      <div>
        <strong>{label}</strong>
        <span>{description}</span>
      </div>
      <code>{shortenPath(value)}</code>
      <div className="syncActions compactActions">
        <button className="pill" onClick={onChoose} type="button">{buttonLabel}</button>
        {onChange && (
          <button className="pill" onClick={() => setShowAdvanced((current) => !current)} type="button">
            {showAdvanced ? "Hide path" : "Edit path manually"}
          </button>
        )}
      </div>
      {showAdvanced && onChange && (
        <input value={value} onChange={(event) => onChange(event.target.value)} />
      )}
    </div>
  );
}
