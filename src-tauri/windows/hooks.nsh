; Ganchos do instalador NSIS do JayV (bundle > windows > nsis > installerHooks).
;
; "Failed to kill JayV": o instalador fecha o app pelo Restart Manager, que
; pede para cada processo do jayv.exe fechar. Três coisas atrapalhavam:
;   - a janela principal só se esconde na bandeja quando pedem para fechar
;     (v0.45.0), então o pedido do Restart Manager não fecha nada;
;   - na atualização pelo app, o atualizador abre o instalador e sai com
;     process::exit, e o instalador já confere enquanto o processo ainda morre;
;   - os agentes que o app abriu (e o `jayv mcp` que o Claude sobe) não morrem
;     com o process::exit, e o `jayv mcp` é o mesmo jayv.exe.
; Antes da conferência do Restart Manager, este gancho espera o JayV terminar
; de sair e, se ainda houver jayv.exe, encerra todos eles. Na instalação à mão ele pergunta antes, como o instalador.

!macro JAYV_RUNNING result
  nsExec::ExecToStack '"$SYSDIR\cmd.exe" /c tasklist /FI "IMAGENAME eq ${MAINBINARYNAME}.exe" /NH | "$SYSDIR\find.exe" /I "${MAINBINARYNAME}.exe"'
  Pop ${result}
  Pop $R8
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro JAYV_RUNNING $R9
  ${If} $R9 == "0"
  ${AndIf} $UpdateMode = 1
    ; O app que pediu a atualização ainda está saindo: até 5 s.
    StrCpy $R7 0
    ${DoWhile} $R9 == "0"
      ${If} $R7 >= 10
        ${ExitDo}
      ${EndIf}
      Sleep 500
      IntOp $R7 $R7 + 1
      !insertmacro JAYV_RUNNING $R9
    ${Loop}
  ${EndIf}
  ${If} $R9 == "0"
    ; Na instalação à mão, com janela, pergunta antes de fechar.
    IfSilent jayv_close
    ${If} $UpdateMode <> 1
    ${AndIf} $PassiveMode <> 1
      nsis_tauri_utils::StrReplace "$(appRunningOkKill)" "{{product_name}}" "${PRODUCTNAME}"
      Pop $R6
      MessageBox MB_OKCANCEL|MB_ICONINFORMATION "$R6" IDOK jayv_close
      nsis_tauri_utils::StrReplace "$(appRunning)" "{{product_name}}" "${PRODUCTNAME}"
      Pop $R6
      Abort "$R6"
    ${EndIf}
    jayv_close:
    nsExec::ExecToStack '"$SYSDIR\taskkill.exe" /F /IM "${MAINBINARYNAME}.exe"'
    Pop $R8
    Pop $R8
    Sleep 1000
  ${EndIf}
!macroend
