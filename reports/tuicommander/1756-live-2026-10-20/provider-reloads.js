const invoke=window.__TAURI__.core.invoke;
const configs={
 'github-work':{accounts:[{id:'1756-github-local',repository:'local/1756'}]},
 gitlab:{accounts:[{id:'1756-gitlab-local',host:'https://gitlab.com',kind:'bearer'}]},
 jira:{accounts:[{id:'1756-jira-local',host:'https://1756.invalid',kind:'bearer'}]}
};
for(const [pluginId,config] of Object.entries(configs))await invoke('write_plugin_data',{pluginId,path:'work-items.json',content:JSON.stringify(config)});
const {i:reload}=await import('/assets/pluginLoader-CqP1WGyp.js');
const {P:providers}=await import('/assets/markdownProviderRegistry-DpO5Z-1F.js');
const results=[];
for(let round=1;round<=2;round++)for(const id of Object.keys(configs)){
 await reload(id);
 const providerId=id==='github-work'?'github':id;
 const registrations=providers.list(providerId).map(p=>({providerId:p.providerId,accountId:p.accountId}));
 if(registrations.length!==1)throw Error(id+' has '+registrations.length+' registrations');
 if(!window.__TUIC__.plugin(id)?.loaded)throw Error(id+' did not load: '+JSON.stringify(window.__TUIC__.plugin(id)));
 results.push({round,id,registrations});
}
return {results,settings:{jira:await invoke('read_plugin_data',{pluginId:'jira',path:'config.json'}),gitlab:await invoke('read_plugin_data',{pluginId:'gitlab',path:'config.json'})},failure:window.__TUIC__.plugin('live-1756-failure')};
