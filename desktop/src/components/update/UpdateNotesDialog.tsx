import { Modal } from "../ui/Modal";
import { UpdateDialogContent } from "./UpdateDialogContent";
import { APP_INFO } from "../../lib/about";
import type { UpdateNotesInfo } from "../../lib/updateNotes";

/** 更新下载行为沿用现有通道；展示层单独解析版本范围。 */
export function UpdateNotesDialog({ open, update, onClose, onDownload, onDismiss }: {
  open: boolean;
  update: UpdateNotesInfo | null;
  onClose: () => void;
  onDownload: () => void;
  onDismiss: () => void;
}) {
  if (!update) return null;
  return <Modal open={open} onClose={onClose} zIndex={1000} ariaLabel={`${APP_INFO.name} 软件更新`} className="modal-surface update-dialog">
    <UpdateDialogContent key={`${update.currentVersion ?? ""}/${update.version ?? ""}`} update={update} onClose={onClose} onDownload={onDownload} onDismiss={onDismiss} />
  </Modal>;
}
