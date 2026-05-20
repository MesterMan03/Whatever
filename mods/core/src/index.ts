import {Console, Message} from "@whatever/api";

Message.registerMessageHandler((_) => {
    return "Hi there! :D";
});

Console.register({
    name: "test",
    description: "Eyy, we're testing :D",
    args: [{
        name: "subcommand",
        description: "Subcommand test :3",
        required: false,
        type: "int"
    }],
    handler: (args) => {
        const subcommandArg = args["subcommand"];
        if(subcommandArg != null) {
            return "You have entered: " + subcommandArg;
        }
        return "Aww, nothing :(";
    }
});