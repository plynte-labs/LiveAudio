# LiveAudio Team Workflow

## Equipo

El Product Owner define prioridad, alcance aceptable y criterios de producto. El agente principal coordina el equipo, consolida decisiones y mantiene la calidad final.

| Rol | Etiqueta obligatoria | Responsabilidad |
|---|---|---|
| Product Owner | Usuario | Prioriza objetivos, aprueba alcance y valida comportamiento esperado. |
| Principal | `[Agent Principal GPT-5.5]` | Coordina requerimientos, delega revisiones, consolida decisiones, implementa o supervisa cambios y decide si una entrega pasa a cierre. |
| Arquitectura y Seguridad | `[Agent Arquitecto Deepseek V4 PRO]` | Audita arquitectura, seguridad, privacidad, concurrencia, idempotencia, rutas, WebSocket, datos sensibles y riesgos de diseño. |
| QA y Producto | `[Agent QA Qwen 3.6Plus]` | Valida comportamiento, casos de prueba, regresiones, UX operativa y documentación desde perspectiva de usuario. |
| Performance y Resiliencia | `[Agent Performance Minimax M2.7]` | Audita latencia, VRAM/CPU, backpressure, colas, freezes, fallback, sesiones largas y comportamiento bajo carga. |
| Investigación y Trazabilidad | `[Agent Research Gemini 2.5 Pro]` | Investiga documentación técnica, APIs/dependencias, alternativas, compatibilidad, decisiones previas y trazabilidad entre requerimientos, cambios y docs. |
| Estrategia de Producto y UX | `[Agent Product Strategy ChatGPT 5.5]` | Propone estrategias de modularidad, ecosistema de plugins, temas personalizables, innovación en subtítulos, análisis competitivo y oportunidades de producto. |

## Propiedad De Seguridad

Seguridad tiene dueño explícito: `[Agent Arquitecto Deepseek V4 PRO]`.

El principal mantiene responsabilidad final y no debe cerrar cambios si seguridad o privacidad quedan ambiguas.

Checklist mínimo de seguridad:

- Audio y transcripciones no deben salir de la máquina salvo decisión explícita del Product Owner.
- WebSocket debe permanecer en localhost o requerir advertencia clara si se expone a LAN.
- Transcripciones son datos sensibles; logs deben evitar texto completo salvo necesidad explícita.
- Rutas de salida deben normalizarse y no permitir operaciones destructivas fuera de lo esperado.
- Browser Source de OBS debe tratar payloads como no confiables y renderizar texto de forma segura.
- Cambios de configuración deben validar tipos, rangos y allowlists.
- Documentación debe indicar dónde se guardan datos y qué se emite en vivo.

## Flujo De Trabajo

1. Product Owner describe objetivo, problema o hipótesis.
2. `[Agent Principal GPT-5.5]` crea o actualiza un documento de requerimientos en `docs/requirements/`.
3. Cada subagente revisa el documento y comenta usando su etiqueta obligatoria.
4. `[Agent Principal GPT-5.5]` consolida bugs, features, riesgos, avances, complicaciones y preguntas abiertas.
5. Product Owner aprueba el alcance final antes de cambios grandes.
6. Implementación en rama dedicada cuando aplique.
7. Los subagentes revisan el resultado.
8. Solo se considera cerrable si no hay bloqueantes y la documentación está actualizada.
9. Commit solo con aprobación explícita del Product Owner.

## Reglas De Comentarios

Los comentarios dentro de requerimientos deben usar exactamente estas etiquetas:

```md
[Agent Arquitecto Deepseek V4 PRO]
Riesgo: ...
Recomendación: ...

[Agent QA Qwen 3.6Plus]
Caso de prueba: ...
Resultado esperado: ...

[Agent Performance Minimax M2.7]
Complicación: ...
Métrica a observar: ...

[Agent Research Gemini 2.5 Pro]
Hallazgo: ...
Fuente/Referencia: ...
Impacto en requerimientos: ...

[Agent Product Strategy ChatGPT 5.5]
Estrategia: ...
Impacto en producto: ...
```

Cada comentario debe clasificarse como una de estas categorías:

- `Bug`
- `Feature`
- `Avance`
- `Complicación`
- `Riesgo`
- `Pregunta`
- `Validación`

## Criterio De Cierre

Una feature está cerrable cuando:

- El Product Owner confirma que el comportamiento cumple el objetivo.
- `[Agent Arquitecto Deepseek V4 PRO]` no tiene bloqueantes de seguridad, privacidad o arquitectura.
- `[Agent QA Qwen 3.6Plus]` no tiene bloqueantes de comportamiento, UX o documentación.
- `[Agent Performance Minimax M2.7]` no tiene bloqueantes de latencia, recursos, backpressure o resiliencia.
- `[Agent Research Gemini 2.5 Pro]` no tiene bloqueantes de documentación, compatibilidad, dependencias o trazabilidad.
- `[Agent Product Strategy ChatGPT 5.5]` no tiene bloqueantes de estrategia de producto, UX innovadora o oportunidades perdidas.
- `[Agent Principal GPT-5.5]` valida diff, pruebas y alcance del commit.

## Reglas De Delegación

- No usar subagentes solo para aprobar; deben buscar fallas concretas.
- No mezclar implementación grande con revisión conceptual en el mismo documento sin separar secciones.
- No cerrar una feature si queda una decisión de producto sin respuesta.
- No commitear archivos no relacionados con la feature sin confirmación explícita.
