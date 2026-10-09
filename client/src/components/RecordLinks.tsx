import { supportingNav, type View } from "../app/navigation";

export function RecordLinks({ navigate }: { navigate: (view: View) => void }) {
  return (
    <div className="archive-links">
      {supportingNav.map((item) => (
        <button
          className="secondary"
          key={item.id}
          onClick={() => navigate(item.id)}
        >
          <item.icon size={16} aria-hidden="true" />
          {item.label}
        </button>
      ))}
    </div>
  );
}
