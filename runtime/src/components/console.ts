import { _send } from "../shared.ts";

/** Public arg type for Console.register(). */
export type ArgType = "string" | "int" | "float" | "bool";

/** Argument specification for a command. */
export type ArgSpec = {
  name: string;
  type: ArgType;
  required?: boolean;
  description?: string;
};

/** A command or subcommand specification. */
export type CommandSpec = {
  name: string;
  description?: string;
  subcommands?: CommandSpec[];
  args?: ArgSpec[];
  handler?: (args: Record<string, string | number | boolean>) => string | string[] | Promise<string | string[]>;
};

type _ArgSpec = {
  name: string;
  type: "string" | "int" | "float" | "bool";
  required: boolean;
  description: string;
};

type _CommandNodeSpec = {
  name: string;
  description: string;
  subcommands: _CommandNodeSpec[];
  args: _ArgSpec[];
  has_handler: boolean;
};

// Command state — handlers keyed by dot-joined path (e.g. "mycmd" or "mycmd.sub").
const _cmdHandlers = new Map<string, NonNullable<CommandSpec["handler"]>>();
const _cmdArgSpecs = new Map<string, _ArgSpec[]>();

function _specToInternal(spec: CommandSpec, pathPrefix: string): _CommandNodeSpec {
  const path = pathPrefix ? `${pathPrefix}.${spec.name}` : spec.name;
  const mappedArgs: _ArgSpec[] = (spec.args ?? []).map((a) => ({
    name: a.name,
    type: a.type,
    required: a.required ?? false,
    description: a.description ?? "",
  }));
  if (spec.handler) {
    _cmdHandlers.set(path, spec.handler);
    _cmdArgSpecs.set(path, mappedArgs);
  }
  return {
    name: spec.name,
    description: spec.description ?? "",
    subcommands: (spec.subcommands ?? []).map((s) => _specToInternal(s, path)),
    args: mappedArgs,
    has_handler: !!spec.handler,
  };
}

/** Called by ipc.ts when a CommandInvoke message arrives. */
export async function _handleCommandInvoke(msg: { request_id: string; command_path: string[]; args: any[] }): Promise<void> {
  const handlerKey = msg.command_path.join(".");
  const handler = _cmdHandlers.get(handlerKey);
  const argSpecs = _cmdArgSpecs.get(handlerKey) ?? [];

  const argsRecord: Record<string, string | number | boolean> = {};
  msg.args.forEach((v, i) => {
    const name = argSpecs[i]?.name ?? String(i);
    argsRecord[name] = v as string | number | boolean;
  });

  if (!handler) {
    _send({ type: "CommandResponse", request_id: msg.request_id, output: [], error: `no handler registered for '${handlerKey}'` });
    return;
  }

  try {
    const result = await handler(argsRecord);
    const lines = Array.isArray(result) ? result : [result];
    _send({ type: "CommandResponse", request_id: msg.request_id, output: lines, error: null });
  } catch (error: unknown) {
    const message = error instanceof Error ? error.message + "\n" + error.stack : String(error);
    _send({ type: "CommandResponse", request_id: msg.request_id, output: [], error: message });
  }
}

/** Register a command that users can invoke from the developer console. */
export const Console = {
  register(spec: CommandSpec): void {
    if (!/^[a-z_]+$/.test(spec.name)) {
      throw new Error(`Console.register: command name '${spec.name}' must match [a-z_]+`);
    }
    const internal = _specToInternal(spec, "");
    _send({
      type: "RegisterCommand",
      name: internal.name,
      description: internal.description,
      subcommands: internal.subcommands,
      args: internal.args,
      has_handler: internal.has_handler,
    });
  },
};
