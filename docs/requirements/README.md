# Requerimientos LiveAudio

Esta carpeta contiene documentos de requerimientos por feature, bugfix o auditoría.

Cada documento debe partir de `TEMPLATE_FEATURE_REQUIREMENTS.md` y mantenerse como fuente de coordinación entre Product Owner, agente principal y subagentes.

## Convención De Nombre

Usar nombres descriptivos en kebab-case:

```txt
YYYY-MM-DD-nombre-de-feature.md
```

Ejemplos:

```txt
2026-05-08-gpu-fallback-ladder.md
2026-05-08-durable-audio-spool.md
2026-05-08-obs-backlog-policy.md
```

## Etiquetas Obligatorias

- `[Agent Principal GPT-5.5]`
- `[Agent Arquitecto Deepseek V4 PRO]`
- `[Agent QA Qwen 3.6Plus]`
- `[Agent Performance Minimax M2.7]`
- `[Agent Research Gemini 2.5 Pro]`

## Qué Debe Quedar Registrado

- Objetivo de producto.
- Alcance aprobado y fuera de alcance.
- Riesgos y decisiones.
- Comentarios de subagentes por nombre.
- Criterios de aceptación.
- Plan de pruebas.
- Estado final: `borrador`, `en revision`, `aprobado`, `implementado`, `cerrado`.
