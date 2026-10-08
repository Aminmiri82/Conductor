import {
  AddRowButton,
  CellInput,
  CheckboxCell,
  HeaderRow,
  RemoveButton,
  Row,
  TableFrame,
} from "@/components/EditableTable";
import type { KeyValue } from "@/features/types";

const columns = "grid-cols-[34px_minmax(120px,0.8fr)_minmax(160px,1.2fr)_34px]";

export function KeyValueTable({
  rows,
  onChange,
  placeholder,
}: {
  rows: KeyValue[];
  onChange: (rows: KeyValue[]) => void;
  placeholder: string;
}) {
  function update(index: number, patch: Partial<KeyValue>) {
    onChange(rows.map((row, i) => (i === index ? { ...row, ...patch } : row)));
  }

  function remove(index: number) {
    onChange(rows.filter((_, i) => i !== index));
  }

  return (
    <TableFrame className="bg-[var(--app-panel-2)]">
      <HeaderRow columns={columns} className="app-mono h-8">
        <div />
        <div>Key</div>
        <div>Value</div>
        <div />
      </HeaderRow>
      {rows.map((row, index) => (
        <Row key={index} columns={columns}>
          <CheckboxCell
            checked={row.enabled ?? true}
            onChange={(enabled) => update(index, { enabled })}
          />
          <CellInput
            value={row.key}
            placeholder={placeholder}
            onChange={(event) => update(index, { key: event.target.value })}
          />
          <CellInput
            value={row.value}
            placeholder="Value"
            onChange={(event) => update(index, { value: event.target.value })}
          />
          <RemoveButton onClick={() => remove(index)} />
        </Row>
      ))}
      <AddRowButton
        onClick={() =>
          onChange([...rows, { key: "", value: "", enabled: true }])
        }
      >
        Add row
      </AddRowButton>
    </TableFrame>
  );
}
