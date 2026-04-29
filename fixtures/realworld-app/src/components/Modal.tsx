export function Modal({ open }: { open: boolean }) {
  return open ? <div className="modal">Modal</div> : null;
}
