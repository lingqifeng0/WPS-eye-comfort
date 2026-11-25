namespace blue {
struct Rgb { int r,g,b; };
static Rgb background={26,43,65},workspace={20,32,50},foreground={222,232,245},muted={120,141,165},accent={115,182,255},hover={44,68,99},border={64,85,112},thumb={117,143,174},card={208,220,236},cardText={18,34,54};
static bool enabled=false;
static bool parseHex(const char* p,DWORD& value){value=0;int count=0;for(;*p;p++){unsigned n=*p>='0'&&*p<='9'?*p-'0':*p>='a'&&*p<='f'?*p-'a'+10:99;if(n>15||++count>8)return false;value=value*16+n;}return count>0;}
static bool setProfile(Request* req){
    const char* names[]={"background","workspace","foreground","muted","accent","hover","border","thumb","card","card_text"};
    Rgb* values[]={&background,&workspace,&foreground,&muted,&accent,&hover,&border,&thumb,&card,&cardText};
    Rgb revised[10];for(int i=0;i<10;i++)revised[i]=*values[i];
    for(DWORD i=0;i<req->count;i++){
        auto& record=req->records[i];DWORD v;int index=-1;
        if(strcmp(record.kind,"palette")||!parseHex(record.value,v)||v>0xffffff)return false;
        for(int j=0;j<10;j++)if(!strcmp(record.name,names[j]))index=j;
        if(index<0)return false;revised[index]={(int)((v>>16)&255),(int)((v>>8)&255),(int)(v&255)};
    }
    for(int i=0;i<10;i++)*values[i]=revised[i];return true;
}
}
