import re

with open('Cargo.toml', 'r') as f:
    content = f.read()

examples = [
    "grpc_client_test",
    "production_deployment",
    "cdc_streaming_example",
    "parquet_streaming_example",
    "parquet_migration_example"
]

for example in examples:
    if f'name = "{example}"' not in content:
        content += f'\n[[example]]\nname = "{example}"\nrequired-features = ["grpc"]\n'

with open('Cargo.toml', 'w') as f:
    f.write(content)
