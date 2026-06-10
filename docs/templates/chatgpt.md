# ChatGPT Free - Prompts de Soporte para Claude Code

## Objetivo

ChatGPT no participa directamente en la implementación del código.

Su función es actuar como:

* revisor técnico
* arquitecto auxiliar
* compresor de contexto
* generador de especificaciones

Claude Code sigue siendo el agente principal de desarrollo.

---

# Revisión de Planes

Utilizar cuando Claude proponga una estrategia de implementación y se quiera una segunda opinión.

## Prompt

Analiza el siguiente plan de implementación.

Objetivos:

* detectar riesgos omitidos
* identificar dependencias olvidadas
* encontrar supuestos débiles
* señalar posibles regresiones
* detectar complejidad innecesaria

No propongas código.

No sugieras refactors fuera del alcance solicitado.

Prioriza observaciones prácticas y accionables.

Plan:

[PEGAR PLAN]

---

# Revisión Arquitectónica

Utilizar antes de realizar cambios importantes.

## Prompt

Analiza la siguiente propuesta arquitectónica.

Evalúa:

* acoplamiento
* cohesión
* mantenibilidad
* compatibilidad futura con upstream
* impacto sobre módulos existentes

Identifica ventajas, riesgos y alternativas.

No generes código.

Propuesta:

[PEGAR PROPUESTA]

---

# Compresión de Current State

Utilizar cuando current-state.md empiece a crecer demasiado.

## Prompt

Reduce el siguiente documento manteniendo únicamente información útil para futuras sesiones.

Conservar:

* decisiones activas
* pendientes
* riesgos
* restricciones

Eliminar:

* detalles históricos
* explicaciones redundantes
* información ya resuelta

El resultado debe ser más corto y más fácil de consumir por Claude Code.

Documento:

[PEGAR CURRENT-STATE]

---

# Generación de Especificación Técnica

Utilizar cuando exista un requerimiento funcional poco claro.

## Prompt

Convierte el siguiente requerimiento funcional en una especificación técnica para Claude Code.

Genera:

* objetivo
* alcance
* restricciones
* criterios de aceptación
* exclusiones explícitas
* riesgos conocidos

No propongas implementación.

Requerimiento:

[PEGAR REQUERIMIENTO]

---

# Revisión de Prompt para Claude

Utilizar cuando una tarea sea compleja y se quiera optimizar el prompt enviado a Claude.

## Prompt

Analiza el siguiente objetivo.

Diseña un prompt para Claude Code que:

* minimice ambigüedad
* limite el alcance
* reduzca trabajo innecesario
* obligue a realizar análisis de impacto antes de modificar código

Objetivo:

[PEGAR OBJETIVO]

---

# Análisis de Riesgo Previo

Utilizar antes de cambios potencialmente peligrosos.

## Prompt

Analiza el siguiente cambio propuesto.

Identifica:

* posibles regresiones
* módulos afectados
* dependencias indirectas
* riesgos para compatibilidad futura
* riesgos para compatibilidad con upstream

Clasifica cada riesgo:

* Bajo
* Medio
* Alto

No propongas código.

Cambio:

[PEGAR CAMBIO]

---

# Generación de ADR (Architecture Decision Record)

Utilizar cuando una decisión merezca ser persistida.

## Prompt

Genera un ADR (Architecture Decision Record) con el siguiente formato:

# Contexto

# Decisión

# Consecuencias

# Alternativas Consideradas

Información:

[PEGAR DECISIÓN]

---

# Regla General

Utilizar ChatGPT para:

* revisar
* cuestionar
* resumir
* estructurar

No utilizar ChatGPT para:

* implementar código del proyecto
* modificar archivos
* reemplazar a Claude Code como agente principal

Claude Code mantiene la responsabilidad de comprender e implementar cambios en el repositorio.
