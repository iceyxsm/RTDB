import re

with open('tests/jepsen_tests.rs', 'r') as f:
    content = f.read()

# I see it missed some configs. I will manually locate and replace them instead of regex.

# Replace line 119-128 completely
config_1 = """    let config = JepsenConfig {
        client_count: 5,
        test_duration_secs: 10,
        operation_rate: 50,
        partition_probability: 0.0,
        enable_simdx: false,
        max_operation_latency_ms: 1000,
        consistency_model: ConsistencyModel::Linearizability,
        ..Default::default()
    };"""

content = re.sub(r'    let config = JepsenConfig \{\s*name: "linearizability-register"[\s\S]*?\.\.Default::default\(\)\s*\};', config_1, content)


# Replace line 178 config completely
config_2 = """    let config = JepsenConfig {
        client_count: 3,
        test_duration_secs: 15,
        operation_rate: 30,
        partition_probability: 0.0,
        enable_simdx: false,
        max_operation_latency_ms: 1000,
        consistency_model: ConsistencyModel::Serializability,
        ..Default::default()
    };"""

content = re.sub(r'    let config = JepsenConfig \{\s*name: "serializability-bank"[\s\S]*?\.\.Default::default\(\)\s*\};', config_2, content)

# Replace line 221 config completely
config_3 = """    let config = JepsenConfig {
        client_count: 4,
        test_duration_secs: 20,
        operation_rate: 25,
        partition_probability: 0.1,
        enable_simdx: false,
        max_operation_latency_ms: 1000,
        consistency_model: ConsistencyModel::Linearizability,
        ..Default::default()
    };"""
content = re.sub(r'    let config = JepsenConfig \{\s*name: "partition-tolerance"[\s\S]*?\.\.Default::default\(\)\s*\};', config_3, content)

# Replace line 281 config completely
config_4 = """    let config = JepsenConfig {
        client_count: 6,
        test_duration_secs: 12,
        operation_rate: 40,
        partition_probability: 0.0,
        enable_simdx: false,
        max_operation_latency_ms: 1000,
        consistency_model: ConsistencyModel::Linearizability,
        ..Default::default()
    };"""
content = re.sub(r'    let config = JepsenConfig \{\s*name: "counter-workload"[\s\S]*?\.\.Default::default\(\)\s*\};', config_4, content)

# Replace line 352 config completely
config_5 = """        let config = JepsenConfig {
            client_count: 3,
            test_duration_secs: 8,
            operation_rate: 30,
            partition_probability: if with_faults { 0.1 } else { 0.0 },
            enable_simdx: false,
            max_operation_latency_ms: 1000,
            consistency_model: consistency,
            ..Default::default()
        };"""
content = re.sub(r'        let config = JepsenConfig \{\s*name: name\.to_string\(\)[\s\S]*?\.\.Default::default\(\)\s*\};', config_5, content)

with open('tests/jepsen_tests.rs', 'w') as f:
    f.write(content)
