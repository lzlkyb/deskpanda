/**
 * modalLayers.ts — 「这一层按键该归谁」的**单一判据**（规则 #11.1）。
 *
 * ## 为什么会冒出这个文件
 *
 * 全站有三类快捷键监听都挂在 `window` 的**捕获期**：设置页（`SettingsView`）、
 * 各个弹框（`useDialogEscape`）、统一确认框（`ConfirmDialog` 自己写的那一份）。
 * 同 target 同相位 → **注册顺序决定谁先跑**，而挂载早就了设置页的监听永远先跑；
 * 它一 `stopPropagation()`，事件根本到不了后来才挂的弹框。
 * （`App.tsx:1067` 那条全局 Esc 分层链是**冒泡期**，所以捕获期一停它就整条断掉。）
 *
 * 修法不能靠「谁后挂谁优先」这种巧合——弹框是用户点开那一刻才挂的，
 * 而每一层都要判「比我高的层在不在场」，判据必须收口在这里，
 * 以后加新一层只改本文件的一处选择器，不用各写各的 `querySelector`。
 *
 * ## 层序（从低到高）
 * 设置页 → 普通弹框（`.dialog-backdrop`，z=400）→ 快捷键浮层（`.shortcut-overlay`，z=400，
 * DOM 更靠后所以盖在设置页上）→ 统一确认框（`.z-confirm`，z=600，专为「从弹框里再弹」设计）
 */

/** 统一确认框在场。它是唯一设计成盖在别的弹框之上的层。 */
export function isConfirmLayerPresent(except?: Element | null): boolean {
  return Array.from(document.querySelectorAll(".z-confirm")).some(layer => layer !== except);
}
