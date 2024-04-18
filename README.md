Go to the folder where you want to create the env and ....

See the tree
secrets-manager config get-all


Get the config of the current path
secrets-manager config get

Create the env file of the config
secrets-manager config export --format env

Put the env into the .env
secrets-manager config export --format env > .env

Set a value

Set a secret
secrets-manager secret set --help

secrets-manager secret --path <PATH> set <KEY> <VALUE>
ex of path: /intercom/dev/ios

Hard coded
secrets-manager config set <NAME_OF_KEY> --value <VALUE> 

through secret
secrets-manager config set <NAME_OF_KEY> --secret <PATH_OF_SECRET> <NAME_OF_KEY>
