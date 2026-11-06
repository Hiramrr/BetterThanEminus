# Tus tareas de Eminus en Muse

Cada persona despliega su propio conector en su cuenta de Vercel. Tu contraseña de Eminus queda en tu proyecto y Muse usa una API key independiente. El conector lee tareas, entregas y material de estudio. No entrega ni modifica actividades.

## Instalar desde una terminal

Necesitas Node.js 22 o posterior, Git y una cuenta de Vercel. Funciona en Windows, macOS y Linux.

```sh
git clone --branch muse-connector https://github.com/Hiramrr/BetterThanEminus.git
cd BetterThanEminus/muse-connector
npm ci
npm run instalar
```

El instalador abre el inicio de sesión de Vercel, crea tu proyecto y pide tu usuario y contraseña de Eminus. La contraseña no aparece mientras escribes. En macOS puedes usar las credenciales que ya tengas en el Llavero.

Después comprueba las credenciales con Eminus, las envía a Vercel como variables sensibles y genera una API key aleatoria. Vercel recibe solo el SHA-256 de esa clave. En macOS la clave queda en el Llavero. En Windows y Linux se muestra una vez en tu terminal para que la guardes en tu gestor de contraseñas.

El comando termina con tu URL, prueba la lectura de tareas y comprueba las instrucciones públicas. Si alguna prueba falla, revisa el error antes de conectar Muse.

## Instalar desde Vercel

[![Desplegar en Vercel](https://vercel.com/button)](https://vercel.com/new/clone?repository-url=https%3A%2F%2Fgithub.com%2FHiramrr%2FBetterThanEminus%2Ftree%2Fmuse-connector%2Fmuse-connector&env=EMINUS_USER,EMINUS_PASSWORD,MUSE_API_KEY&envDescription=Credenciales%20personales%20de%20Eminus%20y%20una%20API%20key%20aleatoria%20para%20Muse&envLink=https%3A%2F%2Fgithub.com%2FHiramrr%2FBetterThanEminus%2Fblob%2Fmuse-connector%2Fmuse-connector%2FREADME.md)

El [botón de Vercel](https://vercel.com/docs/deploy-button) crea una copia en tu cuenta. Comprueba que la carpeta del proyecto contiene `package.json` y `vercel.json` del conector. Si importas el repositorio completo, elige `muse-connector` como directorio raíz y `Other` como framework.

Rellena estas variables en el formulario de Vercel, nunca en el chat de Muse.

| Variable | Valor |
|---|---|
| `EMINUS_USER` | Tu usuario de Eminus |
| `EMINUS_PASSWORD` | Tu contraseña de Eminus |
| `MUSE_API_KEY` | Una clave nueva de al menos 32 caracteres aleatorios, generada y guardada en tu gestor de contraseñas |

Esta opción guarda la API key en Vercel. El instalador de terminal guarda solo su huella. No uses tu contraseña de Eminus como API key.

## Conectar Muse

1. Abre el formulario de conectores de Muse y elige API key.
2. Usa la URL que te dio tu despliegue, por ejemplo `https://tu-proyecto.vercel.app`.
3. Configura la cabecera `Authorization` con el valor `Bearer <tu API key>`.
4. Pasa a tu asistente la dirección `https://tu-proyecto.vercel.app/docs`. Contiene los enlaces a las instrucciones de tareas, avisos y estudio con tu propia URL.

En macOS el instalador muestra un comando para copiar la clave del Llavero al portapapeles sin imprimirla. Pégala solo en el formulario de credenciales de Muse.

No compartas tu clave ni tu URL configurada como si fuera un conector común. Tus compañeros deben usar su propio despliegue y sus propias credenciales. Puedes compartir este README.

## Comprobar y actualizar

Para una prueba de lectura contra tu conector, ejecuta lo siguiente. La API key se pide sin mostrarla.

```sh
node scripts/test-read.mjs --server https://tu-proyecto.vercel.app
```

Para volver a desplegar, ejecuta `npm run instalar`. Si cambiaste la contraseña de Eminus, usa `npm run instalar -- --credentials`. Para generar otra clave de Muse, usa `npm run instalar -- --rotate-key` y actualiza el formulario de Muse.

Si instalaste desde el botón, cambia las variables en tu proyecto de Vercel y vuelve a desplegar. No compartas capturas de sus valores.

Las reglas de avisos están en [AVISOS.md](docs/AVISOS.md). Muse debe ejecutarlas periódicamente y guardar qué avisos envió. El conector por sí solo no envía notificaciones.

## Verificación del código

```sh
npm test
```

Los ejemplos de [examples/](examples/) contienen datos anonimizados. La documentación técnica está en [CONNECTOR.md](CONNECTOR.md). En los archivos locales `{{BASE_URL}}` representa la URL de cada despliegue. La ruta pública `/docs` la sustituye automáticamente.
