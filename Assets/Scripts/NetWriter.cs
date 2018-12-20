using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
using UnityEngine.Networking;
using System.IO;
using System.Text;
using System.Runtime.Serialization.Formatters.Binary;

public class NetWriter : MonoBehaviour
{
    public string netFileName;
    public string nettxtPath;
    public float netFrameLength = 0.1f;
    float netCurrentLength = 0;
    public int PassedFrameNum = -1;
    public int ReceivedFrameNum = 0;
    public int LocalFrameNum = 1;
    public float LocalFrameLength = 0.1f;
    float LocalCurrentLength = 0;
    public List<ClickData> L2S = new List<ClickData>();
    public static int channelID;
    byte[] buffer2s = new byte[1024];
    public bool isstarted = false;
    public byte error;
    LoopList theLL;

    private void FixedUpdate()
    {
        if (!isstarted)
            return;
        netCurrentLength += Time.fixedDeltaTime;
        while (netCurrentLength >= netFrameLength)
        {
            if (theLL.headready())
            {
                theLL.printhead();
                netCurrentLength -= netFrameLength;
                PassedFrameNum++;
                Debug.Log("pfn:" + PassedFrameNum);
            }
            else
            {
                return;
                /*
                 * 此时未接收到远端同一帧的数据
                 * 显示丢帧信息（暂时可通过recordtoggle观察）
                 * 暂停本地游戏（Time.timescale）
                 * 停止记录本地操作（recordtoggle）
                 * 暂停本地NW（isstarted）
                 * 在eat中添加语句，headready时恢复游戏
                 */
            }
        }
    }

    private void Update()
    {
        if (!isstarted)
            return;
        LocalCurrentLength += Time.deltaTime;
        while (LocalCurrentLength >= LocalFrameLength)
        {
            Data2S Fd2s = new Data2S();
            Fd2s.frameNum = LocalFrameNum;
            Fd2s.clientNum = Sender.clientNum;
            Fd2s.clickDatas = L2S;
            BinaryFormatter bf = new BinaryFormatter();
            MemoryStream ms = new MemoryStream();
            bf.Serialize(ms, Fd2s);
            buffer2s = ms.GetBuffer();
            NetworkTransport.Send(Sender.HSID, Sender.CNID, channelID, buffer2s, buffer2s.Length, out error);
            //
            int a = LocalFrameNum - PassedFrameNum - 1;
            theLL.addat(a, Sender.clientNum, L2S);
            //
            L2S.Clear();
            buffer2s = new byte[1024];
            LocalCurrentLength -= LocalFrameLength;
            LocalFrameNum++;
        }
    }

    private void OnEnable()
    {
        theLL = new LoopList();
        theLL.init(GetComponent<ControllerScript>());
        PassedFrameNum = 0;
        ReceivedFrameNum = 0;
        LocalFrameNum = 1;
        isstarted = true;
    }

    private void OnDisable()
    {
        isstarted = false;
        PassedFrameNum = -1;
        ReceivedFrameNum = 0;
        LocalFrameNum = 1;
        //theLL.stop();
    }

    public void Eat(byte[] bRC)
    {
        BinaryFormatter ef = new BinaryFormatter();
        Stream S2E = new MemoryStream(bRC);
        Data2S datarc = (Data2S)ef.Deserialize(S2E);
        ReceivedFrameNum = datarc.frameNum;
        int a = ReceivedFrameNum - PassedFrameNum - 1;
        theLL.addat(a, datarc.clientNum, datarc.clickDatas);
    }
}
public class LoopList
{
    /*FileStream fs;
    StreamWriter sw;*/
    private int headnum;
    private int fullnum;
    private List<ClickData>[,] CDA2;
    private bool[,] bool3;
    private ControllerScript CTL;

    public void addat(int a, int b, List<ClickData> lcd)
    {
        if (a >= fullnum)
            jiabei();
        a += headnum;
        if (a >= fullnum)
            a -= fullnum;
        foreach (ClickData cd in lcd)
        {
            CDA2[a, b].Add(cd);
        }
        bool3[a, b] = true;
        bool3[a, 2] = bool3[a, 0] && bool3[a, 1];
    }

    public bool headready()
    {
        return bool3[headnum, 2];
    }

    public void printhead()
    {
        CTL.powerrr(0, CDA2[headnum, 0]);
        CTL.powerrr(1, CDA2[headnum, 1]);
        for (int n = 0; n < 3; n++)
        {
            bool3[headnum, n] = false;
        }
        headnum++;
        if (headnum >= fullnum)
            headnum -= fullnum;
    }

    void jiabei()
    {
        int nextnum = fullnum * 2;
        int endingnum = fullnum + headnum;
        List<ClickData>[,] nextlist = new List<ClickData>[nextnum, 2];
        bool[,] nextb3 = new bool[nextnum, 3];
        for (int i = headnum; i < endingnum; i++)
        {
            for (int n = 0; n < 2; n++)
            {
                if (i >= fullnum)
                {
                    int m = i - fullnum;
                    nextlist[i, n] = CDA2[m, n];
                    nextb3[i, n] = bool3[m, n];
                }
                else
                {
                    nextlist[i, n] = CDA2[i, n];
                    nextb3[i, n] = bool3[i, n];
                }
            }
        }
        CDA2 = nextlist;
        bool3 = nextb3;
        fullnum = nextnum;
    }

    public void init(ControllerScript TCS)
    {
        //initsw();
        CTL = TCS;
        CTL.createPCs(2);
        headnum = 0;
        fullnum = 32;
        CDA2 = new List<ClickData>[fullnum, 2];
        bool3 = new bool[fullnum, 3];
        for (int i = 0; i < fullnum; i++)
        {
            for (int n = 0; n < 2; n++)
            {
                CDA2[i, n] = new List<ClickData>();
                bool3[i, n] = false;
            }
            bool3[i, 2] = false;
        }
    }

    /*public void stop()
    {
        sw.Close();
        fs.Close();
    }

    void initsw()
    {
        string netFileName = "/" + "L" + string.Format("{0:D2}{1:D2}{2:D2}{3:D2}{4:D2}{5:D2}", System.DateTime.Now.Year, System.DateTime.Now.Month, System.DateTime.Now.Day, System.DateTime.Now.Hour, System.DateTime.Now.Minute, System.DateTime.Now.Second) + ".txt";
        string nettxtPath = Application.dataPath + netFileName;
        fs = new FileStream(nettxtPath, FileMode.OpenOrCreate, FileAccess.Write);
        sw = new StreamWriter(fs);
        sw.WriteLine("Started");
    }

    void PrintList(List<ClickData> theLS)
    {
        foreach (ClickData cd in theLS)
        {
            sw.WriteLine(cd.ToP());
        }
    }*/
}