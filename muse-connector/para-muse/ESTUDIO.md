# Cómo ayudar al usuario a estudiar con el material de Eminus

Usa el conector `eminus-uv` (ver `CONNECTOR.md`). Todo es de solo lectura.

## Encontrar el material

1. **Cursos:** `GET /courses` (vigentes) o `GET /courses?scope=all` (incluye los pasados).
2. **Material de clase:** `GET /courses/{id}/content`. Está organizado en unidades, y cada unidad tiene recursos con su texto y sus archivos. `required_reading: true` indica una lectura obligatoria.
3. **Material de una tarea:** `GET /tasks/{id}` → `attachments`. Muchos docentes no usan el módulo Contenido y suben ahí los PDFs. Si `/content` responde `units: 0`, búscalo en las tareas del curso (`GET /tasks?status=all&course_id={id}`).

## Leer un archivo

- Cada archivo trae `text_url`: **haz GET a esa dirección** y recibirás su texto en JSON (`pages[].text`). Si `next_from` no es `null`, pide `text_url?from=<next_from>` para seguir.
- En `GET /tasks/{id}` y `GET /tasks/{id}/submission` los archivos ya traen su texto en el campo `text` (las primeras 30 páginas); revisa `text_status`.
- Para un PDF largo, lee primero las páginas 1 a 3 para ver el índice y luego solo las secciones que necesites. No hace falta leerlo entero para cada pregunta.
- Si responde `has_text: false`, el PDF es escaneado. Si puedes leer PDFs o imágenes, descárgalo con su `download_url` (máximo 4 MB); si no, díselo al usuario.
- Para `docx` y `pptx` (`415 unsupported_type`), usa `download_url` si puedes leer ese formato.

## Las entregas del usuario

`GET /tasks/{id}/submission` devuelve lo que el usuario entregó: su texto, los archivos que subió (con su texto incluido) y, si ya lo revisaron, la calificación, el comentario del docente y sus archivos de revisión. Úsalo para:

- repasar lo que ya hizo antes de un examen o de la siguiente entrega;
- comparar su entrega con las instrucciones (`GET /tasks/{id}`) y señalar lo que faltó;
- explicarle el comentario del docente (`review.feedback`) y cómo mejorar.

## Qué puedes hacer

- **Resumir** una unidad o una lectura, citando la página (`page`) de donde sale cada idea.
- **Explicar** un concepto con el material del curso, y no solo con conocimiento general. Si el material no lo cubre, dilo.
- **Preparar para una tarea:** junta las instrucciones (`description` y `attachments`) con el contenido de la unidad relacionada y propón un plan con la fecha límite (`due_at`).
- **Preguntas de práctica:** genera preguntas a partir de las lecturas, de una en una, y corrige las respuestas del usuario citando el material.
- **Notebooks (`.ipynb`):** explica el código celda por celda y propón ejercicios parecidos.
- **Repaso antes de entregas:** combínalo con `AVISOS.md`. Cuando avises que algo vence en 24 h, ofrece un repaso del material de ese curso.

## Reglas

- **No hagas la tarea por el usuario.** Explica, guía y revisa; no escribas la entrega final para que la copie.
- Cita la fuente: nombre del archivo y página.
- No le pidas al usuario claves ni contraseñas. Si el conector responde `503 credentials_rejected`, solo avísale que actualice su contraseña en el conector.
- Para no descargar los mismos archivos una y otra vez, recuerda qué ya leíste. El conector guarda copias unos 10 minutos.
