using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
using UnityEngine.Networking;
using System.IO;
using System;
using System.Text;
using System.Runtime.Serialization.Formatters.Binary;
using Bond;
using Steamworks;

public class NetWriter : MonoBehaviour
{
    public Toggle RToggle;
    //public string netFileName;
    //public string nettxtPath;
    public float netFrameLength = 0.06f;
    float netCurrentLength = 0;
    public int PassedFrameNum = -1;
    public int ReceivedFrameNum = 0;
    public int LocalFrameNum = 1;
    public float LocalFrameLength = 0.06f;
    public float LocalCurrentLength = 0;
    public List<ClickData> L2S = new List<ClickData>();
    public static int channelID;
    //byte[] buffer2s;// = new byte[1024];
    public bool isstarted = false;
    //BinaryFormatter bf;
    //public bool Fstarted = false;
    public byte error;
    uint mn = 1;
    public static int rs = 2;
    LoopList theLL = new LoopList();

    private void FixedUpdate()
    {
        /*if (!Fstarted)
            return;*/
        netCurrentLength += Time.fixedDeltaTime;
        while (netCurrentLength >= netFrameLength)
        {
            if (theLL.Numready(mn))
            {
                theLL.printhead();
                netCurrentLength -= netFrameLength;
                PassedFrameNum++;
                mn = 0;
            }
            else
            {
                Time.timeScale /= 2;
                mn = 1;
                Debug.Log("Slow down by TimeScale");
                return;
            }
            Debug.Log("pfn:" + PassedFrameNum);
        }
    }

    private void Update()
    {
        if (!isstarted)
            return;
        if ((LocalFrameNum - PassedFrameNum) < 5)
        {
            if (!RToggle.isOn)
                RToggle.isOn = true;
            LocalCurrentLength += Time.unscaledDeltaTime;
        }
        else
        {
            RToggle.isOn = false;
            LocalCurrentLength += Time.unscaledDeltaTime / 4;
        }
        while (LocalCurrentLength >= LocalFrameLength)
        {
            Data2S Fd2s = new Data2S();
            Fd2s.frameNum = LocalFrameNum;
            Fd2s.clientNum = Sender.clientNum;
            Fd2s.clickDatas = L2S;
            Bond.IO.Safe.OutputBuffer ob = new Bond.IO.Safe.OutputBuffer(64);
            Bond.Protocols.FastBinaryWriter<Bond.IO.Safe.OutputBuffer> bof = new Bond.Protocols.FastBinaryWriter<Bond.IO.Safe.OutputBuffer>(ob);
            Serialize.To(bof, Fd2s);
            ///NetworkTransport.Send(Sender.HSID, Sender.CNID, channelID, ob.Data.Array, ob.Data.Array.Length, out error);
            byte[] sendBytes = new byte[ob.Data.Array.Length + 1];
            sendBytes[0] = (byte)0;
            ob.Data.Array.CopyTo(sendBytes, 1);
            for (int i = 0; i < Sender.TOmb.Length; i++)
            {
                if (i != Sender.clientNum)
                    SteamNetworking.SendP2PPacket(Sender.TOmb[i], sendBytes, (uint)sendBytes.Length, EP2PSend.k_EP2PSendReliable);
            }
            int a = LocalFrameNum - PassedFrameNum - 1;
            theLL.addat(a, Sender.clientNum, L2S);
            Debug.Log("吃！");
            if (Sender.isTesting)
                theLL.addat(a, 1, L2S);
            if (Time.timeScale < 0.6 && theLL.Numready(3))
            {
                //isstarted = false;
                Time.timeScale = 1;
                Debug.Log("TimeScale set to 1 by Update");
            }
            //
            L2S.Clear();
            //buffer2s = new byte[1024];
            LocalCurrentLength -= LocalFrameLength;
            //Debug.Log("Wey: " + LocalFrameNum);
            LocalFrameNum++;
        }
    }

    public void meEnable()
    {
        enabled = true;
        Debug.Log("meEnable");
        //theLL = new LoopList();
        theLL.init(GetComponent<ControllerScript>(), rs);
        PassedFrameNum = 0;
        ReceivedFrameNum = 0;
        LocalFrameNum = 1;
        isstarted = true;
        Time.timeScale = 0;
    }

    public void meDisable()
    {
        isstarted = false;
        PassedFrameNum = -1;
        ReceivedFrameNum = 0;
        LocalFrameNum = 1;
        enabled = false;
        Time.timeScale = 1;
    }

    Data2S bondbfd(byte[] bRC)
    {
        Bond.IO.Safe.InputBuffer ib = new Bond.IO.Safe.InputBuffer(bRC);
        Bond.Protocols.FastBinaryReader<Bond.IO.Safe.InputBuffer> fbr = new Bond.Protocols.FastBinaryReader<Bond.IO.Safe.InputBuffer>(ib);
        Data2S datarc = Deserialize<Data2S>.From(fbr);
        return datarc;
    }

    public void Eat(byte[] bRC)
    {
        Data2S datarc = bondbfd(bRC);//ttt
        ReceivedFrameNum = datarc.frameNum;
        int a = ReceivedFrameNum - PassedFrameNum - 1;
        theLL.addat(a, datarc.clientNum, datarc.clickDatas);
        if (Time.timeScale < 0.6 && theLL.Numready(3))
        {
            //isstarted = false;
            Time.timeScale = 1;
            Debug.Log("TimeScale set to 1 by Eat");
        }
        Debug.Log("Ate");
    }
}
public class LoopList
{
    private int headnum;
    private int fullnum;
    private List<ClickData>[,] CDA2;
    private bool[,] bool3;
    private ControllerScript CTL;
    private int rs;

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
        bool t = true;
        for (int i = 0; i < rs; i++)
        {
            t = t && bool3[a, i];
        }
        bool3[a, rs] = t;
    }

    public bool headready()
    {
        return bool3[headnum, rs];
    }

    public bool Numready(uint u)
    {
        bool a = true;
        for (int i = 0; i <= u; i++)
        {
            a = a && intready(headnum + i);
        }
        return a;
    }

    bool intready(int i)
    {
        if (i >= fullnum)
            i -= fullnum;
        return bool3[i, rs];
    }

    public void printhead()
    {
        int n = 0;
        bool3[headnum, 0] = false;
        while (n < rs)
        {
            CTL.powerrr(n, CDA2[headnum, n]);
            bool3[headnum, ++n] = false;
        }
        headnum++;
        if (headnum >= fullnum)
            headnum -= fullnum;
    }

    void jiabei()
    {
        int nextnum = fullnum * 2;
        int endingnum = fullnum + headnum;
        List<ClickData>[,] CDA2a = new List<ClickData>[nextnum, rs];
        bool[,] bool3a = new bool[nextnum, rs + 1];
        for (int i = 0; i < nextnum; i++)
        {
            for (int n = 0; n < rs; n++)
            {
                CDA2a[i, n] = new List<ClickData>();
                bool3a[i, n] = false;
            }
            bool3a[i, rs] = false;
        }
        for (int i = headnum; i < endingnum; i++)
        {
            for (int n = 0; n < rs; n++)
            {
                if (i >= fullnum)
                {
                    int m = i - fullnum;
                    CDA2a[i, n] = new List<ClickData>(CDA2[m, n]);
                    bool3a[i, n] = bool3[m, n];
                    bool3a[i, rs] = bool3[m, rs];
                }
                else
                {
                    CDA2a[i, n] = new List<ClickData>(CDA2[i, n]);
                    bool3a[i, n] = bool3[i, n];
                    bool3a[i, rs] = bool3[i, rs];
                }
            }
        }
        CDA2 = CDA2a;
        bool3 = bool3a;
        fullnum = nextnum;
    }

    public void init(ControllerScript TCS, int r)
    {
        rs = r;
        CTL = TCS;
        CTL.createPCs(rs);
        headnum = 0;
        fullnum = 32;
        CDA2 = new List<ClickData>[fullnum, rs];
        bool3 = new bool[fullnum, rs + 1];
        for (int i = 0; i < fullnum; i++)
        {
            for (int n = 0; n < rs; n++)
            {
                CDA2[i, n] = new List<ClickData>();
                bool3[i, n] = false;
            }
            bool3[i, rs] = false;
        }
        GameObject.Find("Main Camera").GetComponent<CameraMove>().resetCam();
    }
}