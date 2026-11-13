import type { EngineInterface, Register } from 'claude-code'

const MH = String.raw`(?:^|[\s;&|(\x60'"/\\])mh(?:\.exe)?["']?\s+`
const SECRET_GET = new RegExp(MH + String.raw`secret\s+get\b(?:\s+["']?([^\s"';&|)]+))?`)
const NO_MASK = new RegExp(MH + String.raw`exec\b[^\n;&|]*?\s--no-mask\b`)
const RUN = new RegExp(MH + String.raw`r(?:un)?\s+(?:"([^"]+)"|'([^']+)'|([^\s;&|)]+))`, 'g')
const MENTIONS_MH = /\bmh(?:\.exe)?\b/

type Pipeline = { name: string; steps?: { action: string }[] }

async function pipelinesReadingSecrets($: EngineInterface): Promise<Set<string>> {
  const appdata = await $.env.get('APPDATA')
  const home = await $.env.get('HOME')
  const xdg = await $.env.get('XDG_CONFIG_HOME')
  const dirs = [
    appdata,
    home && `${home}/Library/Application Support`,
    xdg ?? (home && `${home}/.config`),
  ].filter((d): d is string => !!d)
  for (const dir of dirs) {
    const path = `${dir}/magic-hotkey/commands.json`
    if (!(await $.fs.exists(path))) continue
    const { commands } = JSON.parse(await $.fs.read(path)) as { commands: Pipeline[] }
    return new Set(commands.filter(c => c.steps?.some(s => s.action === 'secret')).map(c => c.name))
  }
  return new Set()
}

async function refusal($: EngineInterface, command: string): Promise<string | undefined> {
  if (!MENTIONS_MH.test(command)) return undefined

  const get = SECRET_GET.exec(command)
  if (get) {
    const name = get[1] ?? 'NAME'
    return `\`mh secret get\` would print the value into the conversation. Run the command that needs it as \`mh exec -e VAR=${name} -- <command>\`: it gets the value as $VAR and its output is masked.`
  }

  if (NO_MASK.test(command)) {
    return '`mh exec --no-mask` would let secret values into the conversation. Run it without --no-mask.'
  }

  const runs = [...command.matchAll(RUN)].map(m => m[1] ?? m[2] ?? m[3] ?? '')
  if (runs.length > 0) {
    const secretive = await pipelinesReadingSecrets($)
    const hit = runs.find(name => secretive.has(name))
    if (hit) {
      return `Pipeline "${hit}" reads a secret, so \`mh run\` would print it into the conversation. Pass the secret to the command that needs it with \`mh exec\` instead.`
    }
  }
  return undefined
}

export const register: Register = on => {
  on('tool.call', { tool: 'Bash' }, async ($, e, next) => {
    const reason = await refusal($, e.command)
    return reason ? { deny: `${$.plugin.name}: ${reason}` } : next(e)
  }).catch(($, e, next) => {
    if (next.called || !MENTIONS_MH.test(e.command)) return next(e)
    return { deny: `${$.plugin.name}: could not check this mh command for secret use, so it was not run.` }
  })
}
