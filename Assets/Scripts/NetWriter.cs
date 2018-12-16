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
    /*
    FileStream netfs;
    StreamWriter netSWriter;
    */
    public float netFrameLength = 1f;
    float netCurrentLength = 0;
    public int PassedFrameNum = 0;
    public int ReceivedFrameNum = 1;
    public int LocalFrameNum = 1;
    public float LocalFrameLength = 1f;
    float LocalCurrentLength = 0;
    public List<ClickData> L2S = new List<ClickData>();
    //public byte[] bRC = new byte[1024];
    //public List<ClickData> L2R = new List<ClickData>();
    public static int channelID;
    byte[] buffer2s = new byte[1024];
    public bool isstarted = false;
    public byte error;
    //public List<bool> bp = new List<bool>();
    //public List<List<ClickData>> LLCD = new List<List<ClickData>>(32);
    LoopList theLL;

    private void FixedUpdate()
    {
        if (!isstarted)
            return;
        netCurrentLength += Time.fixedDeltaTime;
        while (netCurrentLength >= netFrameLength && theLL.headready())
        {
            theLL.printhead();
            netCurrentLength -= netFrameLength;
            PassedFrameNum++;
            Debug.Log("pfn:" + PassedFrameNum);
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
            Debug.Log("Local adding at:" + a);
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
        /*
        netFileName = "/" + "net" + string.Format("{0:D2}{1:D2}{2:D2}{3:D2}{4:D2}{5:D2}", System.DateTime.Now.Year, System.DateTime.Now.Month, System.DateTime.Now.Day, System.DateTime.Now.Hour, System.DateTime.Now.Minute, System.DateTime.Now.Second) + ".txt";
        nettxtPath = Application.dataPath + netFileName;
        netfs = new FileStream(nettxtPath, FileMode.OpenOrCreate, FileAccess.ReadWrite);
        netSWriter = new StreamWriter(netfs);
        netSWriter.WriteLine("Started");
        */
        theLL = new LoopList();
        theLL.init();
        isstarted = true;
    }

    private void OnDisable()
    {
        isstarted = false;
        //netSWriter.WriteLine("Stoped");
        //netSWriter.Close();
    }

    /*
    void ta()
    {
        PrintList(LLCD[0]);
        PrintList(LLCD[1]);
        LLCD.RemoveRange(0, 2);
        bp.RemoveRange(0, 3);
    }

    void PrintList(List<ClickData> theLS)
    {
        foreach (ClickData cd in theLS)
        {
            netSWriter.WriteLine(cd.ToP());
        }
    }
    */

    public void Eat(byte[] bRC)
    {
        BinaryFormatter ef = new BinaryFormatter();
        Stream S2E = new MemoryStream(bRC);
        Data2S datarc = (Data2S)ef.Deserialize(S2E);
        ReceivedFrameNum = datarc.frameNum;
        int a = ReceivedFrameNum - PassedFrameNum - 1;
        Debug.Log("Receive adding at:" + a);
        theLL.addat(a, datarc.clientNum, datarc.clickDatas);
    }
}
public class LoopList
{
    FileStream fs;
    StreamWriter sw;
    private int headnum;
    private int fullnum;
    private List<ClickData>[,] CDA2;
    private bool[,] bool3;

    public void addat(int a, int b, List<ClickData> lcd)
    {
        if (a >= fullnum)
            jiabei();
        int ai = a;
        ai += headnum;
        if (ai >= fullnum)
            ai -= fullnum;
        UnityEngine.Debug.Log("adding at:" + ai);
        ai = 0;//调试用
        foreach (ClickData cd in lcd)
        {
            CDA2[ai, b].Add(cd);
        }
        bool3[ai, b] = true;
        bool3[ai, 2] = bool3[ai, 0] && bool3[ai, 1];
    }

    public bool headready()
    {
        return bool3[headnum, 2];
    }

    public void printhead()
    {
        PrintList(CDA2[headnum, 0]);
        PrintList(CDA2[headnum, 1]);
        for (int n = 0; n < 3; n++)
        {
            bool3[headnum, n] = false;
        }
        //headnum++;
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

    public void init()
    {
        initsw();
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

    void initsw()
    {
        string netFileName = "/" + "L" + string.Format("{0:D2}{1:D2}{2:D2}{3:D2}{4:D2}{5:D2}", System.DateTime.Now.Year, System.DateTime.Now.Month, System.DateTime.Now.Day, System.DateTime.Now.Hour, System.DateTime.Now.Minute, System.DateTime.Now.Second) + ".txt";
        string nettxtPath = Application.dataPath + netFileName;
        fs = new FileStream(nettxtPath, FileMode.OpenOrCreate, FileAccess.ReadWrite);
        sw = new StreamWriter(fs);
        sw.WriteLine("Started");
    }

    void PrintList(List<ClickData> theLS)
    {
        foreach (ClickData cd in theLS)
        {
            sw.WriteLine(cd.ToP());
        }
    }
}