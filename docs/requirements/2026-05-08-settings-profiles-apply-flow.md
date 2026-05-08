# Requerimiento: Perfiles De Configuracion Y Aplicar Cambios

Estado: `en revision`
Fecha: `2026-05-08`
Rama sugerida: `feature/settings-profiles-apply-flow`
Product Owner: Usuario
Principal: `[Agent Principal GPT-5.5]`

## Objetivo

Simplificar la configuracion de LiveAudio para que usuarios promedio no tengan que ajustar controles sensibles como si fueran un panel de avion, manteniendo controles avanzados para usuarios expertos.

## Contexto

El proyecto funciona bien, pero ajustar `max atraso`, `pacing`, `silence_timeout` y `max_chunk_duration` puede afectar sensacion de rendimiento si los cambios se aplican mientras el usuario mueve sliders. El Product Owner quiere perfiles preconfigurados, pestañas amigables y un flujo donde los cambios sensibles se guardan/aplican de forma explicita.

## Decisiones Del Product Owner

- Los sliders sensibles deben aplicarse al guardar, no mientras se arrastran.
- Deben existir cuatro perfiles.
- Si el usuario edita un perfil, al guardar se debe preguntar que hacer.
- La configuracion debe organizarse en pestañas amigables.
- Los cambios duros deben aplicarse con hot-swap en vivo, mostrando que puede haber un corte breve.

## Alcance

- Agregar perfiles predefinidos para configuracion rapida.
- Separar valores editados en UI (`draft_config`) de valores activos (`active_config`/`shared_config`) hasta que el usuario pulse `Aplicar cambios`.
- Indicar visualmente cuando hay cambios pendientes.
- Preguntar al aplicar si los cambios modifican un perfil seleccionado: aplicar como personalizado, descartar o cancelar.
- Separar configuracion por categorias/pestañas o estructura equivalente si CustomTkinter limita tabs.
- Mantener compatibilidad con `config.json` existente.

## Fuera De Alcance

- No implementar todavia fallback automatico CUDA/CPU/modelo.
- No implementar cola durable de audio.
- No migrar de CustomTkinter a otro frontend.
- No eliminar configuraciones avanzadas actuales.
- No borrar sesiones ni cambiar retencion de archivos.
- No implementar perfiles personalizados persistentes en el primer corte; los presets integrados no se sobrescriben.

## Perfiles Propuestos

| Perfil | Objetivo | Valores iniciales sugeridos |
|---|---|---|
| `Rápido` | Menor latencia y frases cortas. | `silence_timeout=0.4`, `max_chunk_duration=3.0`, `subtitle_backlog_policy=live_only`, `subtitle_max_live_delay_sec=5.0`, `subtitle_catchup_interval_sec=0.8`, `model_size=base`, `device=cpu` |
| `Balanceado` | Configuracion recomendada para la mayoria. | `silence_timeout=0.8`, `max_chunk_duration=5.0`, `subtitle_backlog_policy=auto`, `subtitle_max_live_delay_sec=10.0`, `subtitle_catchup_interval_sec=1.5`, `model_size=small`, `device=cuda` |
| `Calidad` | Mejor precision con mas tolerancia a latencia. | `silence_timeout=1.0`, `max_chunk_duration=8.0`, `subtitle_backlog_policy=auto`, `subtitle_max_live_delay_sec=15.0`, `subtitle_catchup_interval_sec=2.0`, `model_size=turbo`, `device=cuda` |
| `Streaming estable` | Estabilidad cuando GPU/CPU estan ocupadas por juegos o stream. | `silence_timeout=0.6`, `max_chunk_duration=4.0`, `subtitle_backlog_policy=live_only`, `subtitle_max_live_delay_sec=6.0`, `subtitle_catchup_interval_sec=1.0`, `model_size=small`, `device=cpu` |

Los nombres y valores pueden ajustarse tras revision del equipo.

## Contrato De Configuracion

- `draft_config`: valores visibles/editables en UI; no afectan procesos ni `config.json` hasta aplicar.
- `active_config`: valores activos en la app y procesos.
- `shared_config`: copia compartida usada por procesos en ejecucion; solo se actualiza durante `Aplicar cambios`.
- `persisted_config`: estado guardado en `config.json` tras aplicar con exito.

Regla de consistencia: despues de aplicar correctamente, `draft_config == active_config == persisted_config` para los campos configurables. Si la aplicacion falla, `active_config` y procesos deben quedar en la ultima version valida y la UI debe conservar cambios pendientes o mostrar error.

## Vocabulario UX

- `Aplicar cambios`: valida el draft, activa cambios en la sesion actual y guarda en `config.json` si la aplicacion fue exitosa.
- `Descartar cambios`: restaura el draft desde la configuracion activa.
- `Perfil personalizado`: estado visual cuando el usuario modifica valores de un preset integrado.
- `Guardar como nuevo perfil`: queda fuera del primer corte.

## Trazabilidad De Controles

| Config key | Label UI | Docs |
|---|---|---|
| `selected_profile_id` / `profile_mode` | Perfil de configuración | README: Perfiles de configuración; GETTING_STARTED: Elegir perfil |
| `device` | Hardware | README: Uso básico; GETTING_STARTED: Hardware/modelo |
| `model_size` | Tamaño del Modelo | README: Uso básico; GETTING_STARTED: Hardware/modelo |
| `cpu_threads` | Hilos CPU | README: Configuración por defecto |
| `silence_timeout` | Detección de Silencio | GETTING_STARTED: Ajustar latencia |
| `max_chunk_duration` | Duración máxima de frase | GETTING_STARTED: Ajustar latencia |
| `subtitle_backlog_policy` | Atraso en OBS | README: Política de atraso en OBS; WEBSOCKET_OBS |
| `subtitle_max_live_delay_sec` | Max atraso live | README: Política de atraso en OBS |
| `subtitle_catchup_interval_sec` | Pacing catch-up | README: Política de atraso en OBS |
| `subtitle_style` | Estilo Visual en OBS | WEBSOCKET_OBS |
| `blacklist` | Filtro Anti-Alucinaciones | README: Blacklist predeterminada |

## Cambios Suaves

No deberian requerir hot-swap completo:

- `subtitle_backlog_policy`
- `subtitle_max_live_delay_sec`
- `subtitle_catchup_interval_sec`
- `subtitle_style`
- `blacklist`

## Cambios Duros

Requieren hot-swap en vivo o reinicio parcial:

- `device`
- `model_size`
- `cpu_threads`
- `audio_device`
- `silence_timeout`
- `max_chunk_duration`

## Criterios De Aceptacion

- El usuario puede elegir uno de cuatro perfiles sin entender cada slider.
- Mover sliders no reinicia motor ni cambia configuracion activa hasta aplicar.
- La UI muestra `Cambios pendientes` cuando hay diferencias sin guardar.
- Al aplicar cambios duros, la UI muestra aviso de hot-swap y corte breve.
- Si el usuario edita un perfil integrado, al aplicar se pregunta: aplicar como personalizado, descartar cambios o cancelar.
- Los valores nuevos persisten en `config.json` sin romper configuraciones existentes.
- Usuarios avanzados siguen pudiendo editar valores individuales.
- Los presets integrados no se sobrescriben.
- Si CUDA/modelo/dispositivo requerido no esta disponible, se muestra error recuperable y no se persiste una configuracion que no pudo arrancar.
- Si hay cambios pendientes, cerrar app o cambiar de perfil debe pedir confirmacion.

## Riesgos Iniciales

- CustomTkinter puede complicar tabs/scroll en pantallas bajas.
- Si se difiere todo hasta guardar, `shared_config` debe actualizarse solo al aplicar para evitar estados mixtos.
- Hot-swap sigue pudiendo perder frase en curso si se cambia VAD/ASR durante habla activa.
- Valores de perfiles pueden no ser ideales para todos los equipos.
- El primer corte no resuelve perdida de frase en curso durante hot-swap; debe avisarse al usuario.

## Comentarios Del Equipo

### Arquitectura Y Seguridad

```md
[Agent Arquitecto Deepseek V4 PRO]
Categoria: Riesgo
Severidad: Alta
Comentario: La separacion `draft_config` vs configuracion activa debe ser contractual. Mover sliders o cambiar selects solo debe modificar draft; `shared_config`, procesos y `config.json` no deben mutar hasta `Aplicar cambios`.
Recomendacion: Definir `draft_config`, `active_config`, `shared_config` y `persisted_config`, con rollback si falla la aplicacion.
Bloqueante: resuelto en requerimiento; validar en implementacion.
```

```md
[Agent Arquitecto Deepseek V4 PRO]
Categoria: Riesgo
Severidad: Alta
Comentario: Hot-swap necesita semantica precisa: bloquear doble aplicacion, mostrar estado aplicando, reiniciar solo una vez por lote, conservar sesion y registrar error si falla.
Recomendacion: Primer corte debe advertir corte breve y no persistir si una configuracion dura no pudo arrancar.
Bloqueante: parcialmente resuelto; perdida de frase en curso queda como riesgo conocido fuera de alcance.
```

### QA Y Producto

```md
[Agent QA Qwen 3.6Plus]
Categoria: Validacion
Severidad: Media
Comentario: La UI debe distinguir `Perfil seleccionado`, `Configuracion activa` y `Cambios pendientes`. El boton debe llamarse `Aplicar cambios` o `Aplicar y guardar`, no solo guardar.
Recomendacion: Usar textos humanos y confirmar al cambiar perfil/cerrar con cambios pendientes.
Bloqueante: resuelto en requerimiento; validar en UI.
```

```md
[Agent QA Qwen 3.6Plus]
Categoria: Feature
Severidad: Media
Comentario: `Streaming Pesado` puede sonar ambiguo.
Recomendacion: Usar `Streaming estable` como nombre visible.
Bloqueante: resuelto.
```

### Performance Y Resiliencia

```md
[Agent Performance Minimax M2.7]
Categoria: Riesgo
Severidad: Alta
Comentario: Cambios suaves y duros deben separarse de cuando se aplican. Incluso cambios suaves deben quedar pendientes hasta `Aplicar cambios`, aunque no requieran hot-swap.
Recomendacion: Clasificar duros en ASR (`device`, `model_size`, `cpu_threads`) y Audio/VAD (`audio_device`, `silence_timeout`, `max_chunk_duration`). Aplicar varios cambios duros con un solo hot-swap.
Bloqueante: resuelto en requerimiento; validar en implementacion.
```

```md
[Agent Performance Minimax M2.7]
Categoria: Complicacion
Severidad: Media
Comentario: `Calidad` con CUDA/turbo debe advertir consumo alto de VRAM. `Streaming estable` en CPU/small puede ser pesado en equipos ocupados.
Recomendacion: Primer corte debe mostrar descripcion clara de cada perfil y dejar que el usuario ajuste manualmente.
Bloqueante: no.
```

### Investigacion Y Trazabilidad

```md
[Agent Research Gemini 2.5 Pro]
Categoria: Riesgo
Severidad: Alta
Comentario: Los perfiles deben usar valores canonicos compatibles con `utils/config.py`, que guarda `model_size` como etiqueta descriptiva completa.
Recomendacion: Definir perfiles con las mismas cadenas que la UI/config actual usa, no claves cortas ambiguas.
Bloqueante: resuelto en implementacion requerida.
```

```md
[Agent Research Gemini 2.5 Pro]
Categoria: Validacion
Severidad: Media
Comentario: Falta tabla de trazabilidad `config key -> label UI -> docs`.
Recomendacion: Documentar labels finales en README/GETTING_STARTED y mantener config antigua migrable.
Bloqueante: no.
```

## Decisiones

| Decision | Responsable | Fecha | Motivo |
|---|---|---|---|
| Cambios sensibles se aplican al guardar | Product Owner | 2026-05-08 | Evitar lag/reinicios mientras se arrastran sliders. |
| Usar cuatro perfiles | Product Owner | 2026-05-08 | Cubrir rapido, balanceado, calidad y streaming pesado. |
| Hot-swap en vivo con aviso | Product Owner | 2026-05-08 | Mantener continuidad de sesion con corte breve esperado. |

## Plan De Implementacion

- Agregar presets y config de perfil actual/personalizado.
- Separar `draft_config` de `config_data` activa en UI.
- Agregar boton `Aplicar cambios` y estado de cambios pendientes.
- Implementar pregunta al aplicar si el perfil fue modificado.
- Agrupar controles en pestañas amigables o frames equivalentes.
- Actualizar documentacion.

## Plan De Pruebas

- Automatico: `python -m compileall main.py core utils`.
- Manual: elegir cada perfil, editar sliders, aplicar, descartar y reiniciar app.
- Hot-swap: cambiar perfil con motor activo y confirmar aviso/corte breve.
- OBS/WebSocket: confirmar que cambios suaves no reinician ASR.
- Sesion larga: confirmar que aplicar cambios no borra ruta ni sesion activa.
- Error: cancelar confirmacion, aplicar dos veces rapido, intentar CUDA sin disponibilidad, cerrar con cambios pendientes y usar config antigua sin claves de perfil.

## Documentacion A Actualizar

- `README.md`
- `docs/GETTING_STARTED.md`
- `HISTORIAL_CAMBIOS.md`
- `docs/WEBSOCKET_OBS.md` si cambian controles OBS/backlog.

## Estado Final

Veredicto: `aprobado para primer corte de implementacion`

Pendientes:

- Implementar primer corte sin perfiles personalizados persistentes.
- Revision final de los cuatro subagentes tras implementacion.
