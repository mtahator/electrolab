; Installer steps added to the generated NSIS installer (tauri.conf.json: bundle.windows.nsis).
;
; The layout model (figures, tables and equations; about 78 MB) is optional (ADR-013). The
; installer asks once; on "Yes" it leaves a flag next to the app, and the app downloads the
; model on its first start, verified against its pinned checksum, with progress in the status
; line. A silent install (/S) answers "No". The model can also be got or removed later in
; Settings.

!macro NSIS_HOOK_POSTINSTALL
  MessageBox MB_YESNO|MB_ICONQUESTION "Also get the layout model (about 78 MB)?$\r$\n$\r$\nIt finds figures, tables and equations on every page, so you can jump to them and open them in place. It runs on this computer; nothing about your papers is sent anywhere.$\r$\n$\r$\nIt is downloaded the first time the Reader starts. You can also get it or remove it later in Settings." /SD IDNO IDNO ep_skip_layout_model
    FileOpen $0 "$INSTDIR\get-layout-model" w
    FileClose $0
  ep_skip_layout_model:
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  Delete "$INSTDIR\get-layout-model"
!macroend
