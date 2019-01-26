using System.Collections;
using System.Collections.Generic;
using UnityEngine;
///using Photon;

public class SkillE2b : MonoBehaviour
{
    public CooldownImage MyImageScript;
    //public float SpeedR2;
    public float maxTimeE2;
    public float pushPower;
    public float pushTime;
    public float pushDamage;
    private float currentcooldown;
    public float cooldowntime = 5;
    public bool skillavaliable;
    public bool working = false;
    bool reworkb = false;
    float rwtime = 0;
    float worktime;
    MoveScript MS;

    // Use this for initialization
    void Start()
    {
        currentcooldown = cooldowntime;
        MS = GetComponent<MoveScript>();
    }

    // Update is called once per frame
    void GoSkillE2b()
    {
        if (skillavaliable && GetComponent<DoSkill>().CanSing)
        {
            GetComponent<DoSkill>().singing = 0;
            Skill();
        }
    }

    private void FixedUpdate()
    {
        if (working)
        {
            worktime += Time.fixedDeltaTime;
            if (worktime >= maxTimeE2)
                working = false;
        }
        if (reworkb)
        {
            rwtime += Time.fixedDeltaTime;
            if (rwtime >= 0.3f)
            {
                reworkb = false;
                mywork();
            }
        }
        if (skillavaliable)
            return;
        if (currentcooldown >= cooldowntime)
        {
            skillavaliable = true;
        }
        else
        {
            currentcooldown += Time.fixedDeltaTime;
        }
    }

    public void Skill()
    {
        GetComponent<DoSkill>().BeforeSkill();
        MS.controllable = true;
        currentcooldown = 0;
        skillavaliable = false;
        working = true;
        mywork();
    }

    void mywork()
    {
        gameObject.GetComponent<ColliderScript>().SetPower(pushPower, pushTime, (FixMath.Fix64)pushDamage);
        gameObject.GetComponent<ColliderScript>().StartKick(maxTimeE2);
        gameObject.GetComponent<StealthScript>().StealthByTime(maxTimeE2, false);
        gameObject.GetComponent<StealthScript>().Speed = 1;
        worktime = 0;
    }

    void OnCollisionEnter2D(Collision2D collision)
    {
        if (!collision.gameObject.CompareTag("Player"))
            working = false;
        if (!working)
            return;
        reworkb = true;
    }

    public void lighthit()
    {
        if (!working && !reworkb)
            return;
        GetComponent<ColliderScript>().StopKick();
        working = false;
        reworkb = false;
    }

    void SkillE2bSetLevel(int i)
    {
        if (i == 0)
            enabled = false;
        else
            enabled = true;
    }

    public float CalcFA()
    {
        return currentcooldown / cooldowntime;
    }
}
