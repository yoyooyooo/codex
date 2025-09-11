#!/usr/bin/env node

/**
 * 自动化网络环境测试脚本
 * 用于在不同代理和网络配置下验证 WebSocket 和 SSE 的兼容性
 */

const https = require('https');
const http = require('http');
const WebSocket = require('ws');
const EventSource = require('eventsource');
const { performance } = require('perf_hooks');

class NetworkCompatibilityTest {
    constructor(serverUrl = 'http://localhost:3000') {
        this.serverUrl = serverUrl;
        this.results = {
            environments: {},
            summary: {}
        };
    }

    // 测试配置矩阵
    getTestConfigurations() {
        return [
            {
                name: 'direct',
                description: '直连测试',
                config: { proxy: null, timeout: 5000 }
            },
            {
                name: 'http-proxy',
                description: 'HTTP代理测试',
                config: { 
                    proxy: { host: 'localhost', port: 8080, protocol: 'http' },
                    timeout: 10000
                }
            },
            {
                name: 'https-proxy',
                description: 'HTTPS代理测试', 
                config: { 
                    proxy: { host: 'localhost', port: 8443, protocol: 'https' },
                    timeout: 10000
                }
            },
            {
                name: 'slow-network',
                description: '慢速网络测试',
                config: { proxy: null, timeout: 30000, delay: 2000 }
            }
        ];
    }

    // WebSocket 兼容性测试
    async testWebSocketCompatibility(config) {
        return new Promise((resolve) => {
            const results = {
                connectionSuccess: false,
                connectionTime: null,
                messageLatency: [],
                errors: [],
                reconnectSuccess: false
            };

            const startTime = performance.now();
            const wsUrl = this.serverUrl.replace('http', 'ws') + '/ws';
            
            let ws;
            try {
                // 配置 WebSocket 连接选项
                const options = {
                    timeout: config.timeout || 5000,
                };

                if (config.proxy) {
                    // 在实际部署中，这里需要配置代理
                    options.agent = this.createProxyAgent(config.proxy);
                }

                ws = new WebSocket(wsUrl, options);

                ws.on('open', () => {
                    const connectionTime = performance.now() - startTime;
                    results.connectionSuccess = true;
                    results.connectionTime = connectionTime;

                    // 发送测试消息
                    const testMessage = {
                        id: `test-${Date.now()}`,
                        timestamp: performance.now(),
                        content: 'WebSocket compatibility test',
                        message_type: 'Ping'
                    };

                    ws.send(JSON.stringify(testMessage));
                });

                ws.on('message', (data) => {
                    try {
                        const message = JSON.parse(data.toString());
                        const latency = performance.now() - message.timestamp;
                        results.messageLatency.push(latency);
                    } catch (e) {
                        results.errors.push(`Message parse error: ${e.message}`);
                    }
                });

                ws.on('error', (error) => {
                    results.errors.push(error.message);
                });

                ws.on('close', () => {
                    // 测试重连
                    setTimeout(() => {
                        try {
                            const reconnectWs = new WebSocket(wsUrl, options);
                            reconnectWs.on('open', () => {
                                results.reconnectSuccess = true;
                                reconnectWs.close();
                                resolve(results);
                            });
                            reconnectWs.on('error', () => {
                                resolve(results);
                            });
                        } catch (e) {
                            resolve(results);
                        }
                    }, 1000);
                });

                // 测试超时
                setTimeout(() => {
                    if (ws.readyState === WebSocket.CONNECTING) {
                        results.errors.push('Connection timeout');
                        ws.terminate();
                        resolve(results);
                    }
                }, config.timeout || 5000);

            } catch (error) {
                results.errors.push(error.message);
                resolve(results);
            }

            // 5秒后关闭连接进行重连测试
            setTimeout(() => {
                if (ws && ws.readyState === WebSocket.OPEN) {
                    ws.close();
                }
            }, 5000);
        });
    }

    // SSE 兼容性测试
    async testSSECompatibility(config) {
        return new Promise((resolve) => {
            const results = {
                connectionSuccess: false,
                connectionTime: null,
                messageLatency: [],
                errors: [],
                reconnectSuccess: false
            };

            const startTime = performance.now();
            const sseUrl = this.serverUrl + '/sse';

            try {
                const eventSourceOptions = {
                    timeout: config.timeout || 5000
                };

                if (config.proxy) {
                    eventSourceOptions.proxy = config.proxy;
                }

                const eventSource = new EventSource(sseUrl, eventSourceOptions);

                eventSource.onopen = () => {
                    const connectionTime = performance.now() - startTime;
                    results.connectionSuccess = true;
                    results.connectionTime = connectionTime;

                    // 发送测试消息（通过 HTTP POST）
                    this.sendSSETestMessage();
                };

                eventSource.onmessage = (event) => {
                    try {
                        const message = JSON.parse(event.data);
                        const latency = performance.now() - message.timestamp;
                        results.messageLatency.push(latency);
                    } catch (e) {
                        results.errors.push(`Message parse error: ${e.message}`);
                    }
                };

                eventSource.onerror = (error) => {
                    results.errors.push('SSE connection error');
                    
                    // 测试重连
                    setTimeout(() => {
                        try {
                            const reconnectES = new EventSource(sseUrl, eventSourceOptions);
                            reconnectES.onopen = () => {
                                results.reconnectSuccess = true;
                                reconnectES.close();
                                resolve(results);
                            };
                            reconnectES.onerror = () => {
                                resolve(results);
                            };
                        } catch (e) {
                            resolve(results);
                        }
                    }, 1000);
                };

                // 测试超时
                setTimeout(() => {
                    results.errors.push('Test timeout');
                    eventSource.close();
                    resolve(results);
                }, (config.timeout || 5000) + 10000);

                // 10秒后关闭连接
                setTimeout(() => {
                    eventSource.close();
                }, 10000);

            } catch (error) {
                results.errors.push(error.message);
                resolve(results);
            }
        });
    }

    // 发送 SSE 测试消息
    async sendSSETestMessage() {
        try {
            const message = {
                message_type: 'Broadcast',
                content: 'SSE compatibility test'
            };

            const response = await fetch(this.serverUrl + '/test', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(message)
            });

            return await response.json();
        } catch (e) {
            console.error('Failed to send SSE test message:', e);
        }
    }

    // 创建代理代理（简化版）
    createProxyAgent(proxyConfig) {
        if (proxyConfig.protocol === 'http') {
            const HttpProxyAgent = require('http-proxy-agent');
            return new HttpProxyAgent(`http://${proxyConfig.host}:${proxyConfig.port}`);
        } else if (proxyConfig.protocol === 'https') {
            const HttpsProxyAgent = require('https-proxy-agent');
            return new HttpsProxyAgent(`https://${proxyConfig.host}:${proxyConfig.port}`);
        }
        return null;
    }

    // 运行完整测试套件
    async runFullTestSuite() {
        console.log('🚀 开始 WebSocket vs SSE 兼容性测试套件...\n');

        const configurations = this.getTestConfigurations();

        for (const config of configurations) {
            console.log(`📊 测试环境: ${config.description}`);
            
            // WebSocket 测试
            console.log('  🔌 WebSocket 测试中...');
            const wsResults = await this.testWebSocketCompatibility(config.config);
            
            // SSE 测试
            console.log('  📡 SSE 测试中...');
            const sseResults = await this.testSSECompatibility(config.config);

            // 保存结果
            this.results.environments[config.name] = {
                description: config.description,
                webSocket: wsResults,
                sse: sseResults
            };

            // 输出结果
            this.printEnvironmentResults(config.name, wsResults, sseResults);
            console.log('');
        }

        // 生成总结报告
        this.generateSummaryReport();
        this.printRecommendations();
    }

    // 输出单个环境的测试结果
    printEnvironmentResults(envName, wsResults, sseResults) {
        console.log(`  结果 - ${envName.toUpperCase()}:`);
        
        // WebSocket 结果
        console.log(`    WebSocket:`);
        console.log(`      连接成功: ${wsResults.connectionSuccess ? '✅' : '❌'}`);
        console.log(`      连接时间: ${wsResults.connectionTime ? wsResults.connectionTime.toFixed(2) + 'ms' : 'N/A'}`);
        console.log(`      平均延迟: ${wsResults.messageLatency.length > 0 ? (wsResults.messageLatency.reduce((a, b) => a + b, 0) / wsResults.messageLatency.length).toFixed(2) + 'ms' : 'N/A'}`);
        console.log(`      重连成功: ${wsResults.reconnectSuccess ? '✅' : '❌'}`);
        console.log(`      错误数量: ${wsResults.errors.length}`);
        
        // SSE 结果
        console.log(`    SSE:`);
        console.log(`      连接成功: ${sseResults.connectionSuccess ? '✅' : '❌'}`);
        console.log(`      连接时间: ${sseResults.connectionTime ? sseResults.connectionTime.toFixed(2) + 'ms' : 'N/A'}`);
        console.log(`      平均延迟: ${sseResults.messageLatency.length > 0 ? (sseResults.messageLatency.reduce((a, b) => a + b, 0) / sseResults.messageLatency.length).toFixed(2) + 'ms' : 'N/A'}`);
        console.log(`      重连成功: ${sseResults.reconnectSuccess ? '✅' : '❌'}`);
        console.log(`      错误数量: ${sseResults.errors.length}`);
    }

    // 生成总结报告
    generateSummaryReport() {
        const summary = {
            webSocket: { success: 0, total: 0, avgLatency: 0, reconnectRate: 0 },
            sse: { success: 0, total: 0, avgLatency: 0, reconnectRate: 0 }
        };

        let wsLatencies = [];
        let sseLatencies = [];
        let wsReconnects = 0;
        let sseReconnects = 0;

        Object.values(this.results.environments).forEach(env => {
            // WebSocket 统计
            summary.webSocket.total++;
            if (env.webSocket.connectionSuccess) {
                summary.webSocket.success++;
            }
            if (env.webSocket.reconnectSuccess) {
                wsReconnects++;
            }
            wsLatencies = wsLatencies.concat(env.webSocket.messageLatency);

            // SSE 统计
            summary.sse.total++;
            if (env.sse.connectionSuccess) {
                summary.sse.success++;
            }
            if (env.sse.reconnectSuccess) {
                sseReconnects++;
            }
            sseLatencies = sseLatencies.concat(env.sse.messageLatency);
        });

        // 计算平均值
        summary.webSocket.avgLatency = wsLatencies.length > 0 ? 
            wsLatencies.reduce((a, b) => a + b, 0) / wsLatencies.length : 0;
        summary.sse.avgLatency = sseLatencies.length > 0 ? 
            sseLatencies.reduce((a, b) => a + b, 0) / sseLatencies.length : 0;
        
        summary.webSocket.reconnectRate = summary.webSocket.total > 0 ? 
            wsReconnects / summary.webSocket.total * 100 : 0;
        summary.sse.reconnectRate = summary.sse.total > 0 ? 
            sseReconnects / summary.sse.total * 100 : 0;

        this.results.summary = summary;
    }

    // 输出建议
    printRecommendations() {
        console.log('📋 测试总结报告:');
        console.log('=====================================');
        
        const ws = this.results.summary.webSocket;
        const sse = this.results.summary.sse;
        
        console.log(`WebSocket 总体表现:`);
        console.log(`  成功率: ${(ws.success/ws.total*100).toFixed(1)}% (${ws.success}/${ws.total})`);
        console.log(`  平均延迟: ${ws.avgLatency.toFixed(2)}ms`);
        console.log(`  重连成功率: ${ws.reconnectRate.toFixed(1)}%`);
        
        console.log(`\nSSE 总体表现:`);
        console.log(`  成功率: ${(sse.success/sse.total*100).toFixed(1)}% (${sse.success}/${sse.total})`);
        console.log(`  平均延迟: ${sse.avgLatency.toFixed(2)}ms`);
        console.log(`  重连成功率: ${sse.reconnectRate.toFixed(1)}%`);
        
        console.log('\n🎯 技术选择建议:');
        console.log('=====================================');
        
        const wsSuccessRate = ws.success/ws.total*100;
        const sseSuccessRate = sse.success/sse.total*100;
        
        if (wsSuccessRate >= 85 && ws.reconnectRate >= 90) {
            console.log('✅ 推荐方案: 仅使用 WebSocket');
            console.log('   理由: WebSocket 在所有测试环境下表现良好');
            console.log('   实现复杂度: 低');
        } else if (wsSuccessRate >= 70 && sseSuccessRate >= 85) {
            console.log('⚠️  推荐方案: WebSocket + SSE 自动降级');
            console.log('   理由: WebSocket 基本可用，SSE 作为备选方案');
            console.log('   实现复杂度: 中等');
            console.log('   降级触发条件:');
            console.log('     - WebSocket 连接失败');
            console.log('     - WebSocket 消息传输错误率 > 10%');
        } else if (sseSuccessRate >= 85) {
            console.log('🔄 推荐方案: 优先使用 SSE');
            console.log('   理由: SSE 兼容性更好，WebSocket 存在问题');
            console.log('   实现复杂度: 低到中等');
        } else {
            console.log('❌ 警告: 两种方案都存在兼容性问题');
            console.log('   建议: 重新评估技术方案或网络环境配置');
        }

        console.log('\n📊 详细环境分析:');
        console.log('=====================================');
        Object.entries(this.results.environments).forEach(([envName, data]) => {
            const wsSuccess = data.webSocket.connectionSuccess;
            const sseSuccess = data.sse.connectionSuccess;
            
            if (!wsSuccess && sseSuccess) {
                console.log(`⚠️  ${data.description}: WebSocket 失败，SSE 正常 - 需要降级机制`);
            } else if (wsSuccess && !sseSuccess) {
                console.log(`ℹ️  ${data.description}: WebSocket 正常，SSE 失败 - WebSocket 优先`);
            } else if (!wsSuccess && !sseSuccess) {
                console.log(`❌ ${data.description}: 两种协议都失败 - 环境问题`);
            } else {
                console.log(`✅ ${data.description}: 两种协议都正常`);
            }
        });
    }

    // 导出测试结果
    exportResults(filename = 'websocket-sse-test-results.json') {
        const fs = require('fs');
        const timestamp = new Date().toISOString();
        const exportData = {
            timestamp,
            testVersion: '1.0.0',
            serverUrl: this.serverUrl,
            ...this.results
        };
        
        fs.writeFileSync(filename, JSON.stringify(exportData, null, 2));
        console.log(`\n💾 测试结果已导出到: ${filename}`);
    }
}

// 运行测试
if (require.main === module) {
    const serverUrl = process.argv[2] || 'http://localhost:3000';
    const test = new NetworkCompatibilityTest(serverUrl);
    
    test.runFullTestSuite().then(() => {
        test.exportResults();
        process.exit(0);
    }).catch(error => {
        console.error('❌ 测试失败:', error);
        process.exit(1);
    });
}

module.exports = NetworkCompatibilityTest;