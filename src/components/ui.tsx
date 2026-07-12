import { ReactNode } from "react";

export function Avatar({ name, q }: { name: string; q?: boolean }) {
  return <div className={q ? "avt q" : "avt"}>{name ? name[0] : "?"}</div>;
}

export function Modal({
  title,
  onClose,
  children,
  footer,
  small,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  small?: boolean;
}) {
  return (
    <div className="mask" onClick={onClose}>
      <div className={small ? "modal sm" : "modal"} onClick={(e) => e.stopPropagation()}>
        <div className="mh">
          <h3>{title}</h3>
          <button className="x" onClick={onClose}>
            ×
          </button>
        </div>
        <div className="mb">{children}</div>
        {footer && <div className="mf">{footer}</div>}
      </div>
    </div>
  );
}
