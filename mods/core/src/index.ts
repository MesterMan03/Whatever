import {Console, Message} from "@whatever-engine/api";

Message.registerMessageHandler((_) => {
    return "Hi there! :D";
});

Console.register({
    name: "test",
    description: "Eyy, we're testing :D",
    subcommands: [{
        name: "subcommand",
        description: "Subcommand test :3",
        args: [{
            name: "value",
            description: "Test value !!",
            required: true,
            type: "string"
        }],
        handler: (args) => {
            const value = args["value"] as string;
            return "You have entered: " + value;
        }
    }],
    handler: (_) => {
        return "Nope, try again with the subcommand <3";
    }
});

Console.register({
    name: "illegal",
    description: "Triggers a runtime crash to test how the engine responds",
    handler: (_) => {
        const illegal = "oopsies";
        // @ts-ignore
        return "You're not supposed to see this: " + illegal.lmao.noway;
    }
});