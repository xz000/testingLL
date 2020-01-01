using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class DoSkill : MonoBehaviour
{
    public int singing;
    public delegate void PointSkill(Fix64Vector2 actionplace);
    public PointSkill Fire;
    public delegate void NoPointSkill();
    public NoPointSkill ClearDebuff;
    public NoPointSkill WorkBeforeDestroy;
    public bool CanSing = true;
    float timetostop;
    float timepassed;

    // Use this for initialization
    void Start()
    {
        Fire = null;
        ClearDebuff = null;
    }

    private void FixedUpdate()
    {
        if (CanSing)
            return;
        if (timepassed >= timetostop)
        {
            CanSing = true;
        }
        else
            timepassed += Time.fixedDeltaTime;
    }

    public void GetTied(float time)
    {
        CanSing = false;
        Fire = null;
        timepassed = 0;
        timetostop = time;
    }

    public void justdoit(Fix64Vector2 fv2)
    {
        if (Fire == null)
            return;
        BeforeSkill();
        Fire(fv2);
        Debug.Log("Just Fired to " + fv2.ToV2().x);
        FireReset();
    }

    public void GoFireStop()
    {
        gameObject.GetComponent<MoveScript>().stopwalking();
        singing = 0;
        FireReset();
    }

    public void FireReset()
    {
        Fire = null;
    }

    public void DoClearJob()
    {
        if (ClearDebuff == null)
            return;
        ClearDebuff();
        ClearDebuff = null;
    }

    public void DestroyClean()
    {
        if (WorkBeforeDestroy == null)
            return;
        WorkBeforeDestroy();
        WorkBeforeDestroy = null;
    }

    public void BeforeSkill()
    {
        gameObject.GetComponent<MoveScript>().stopwalking(); //停止走动
        gameObject.GetComponent<StealthScript>().StealthEnd();
        gameObject.GetComponent<SkillE2b>().lighthit();
    }
}
